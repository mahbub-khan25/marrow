//! Markdown → note metadata.
//!
//! Extracts everything the rest of the app needs without re-reading a note:
//! wikilinks and Markdown links, embeds, tags, headings, list items and
//! tasks, `^block` ids and YAML frontmatter, each with its exact position.
//!
//! CommonMark structure (code, headings, lists, links) comes from
//! `pulldown-cmark`. Wikilinks, tags and block ids are found by a small
//! scanner over the raw text that skips code, frontmatter and `%% comments %%`.

use std::collections::BTreeMap;
use std::ops::Range;

use pulldown_cmark::{Event, MetadataBlockKind, Options, Parser, Tag, TagEnd};
use serde::{Deserialize, Serialize};

/// A point in the note: zero-based line, byte column, byte offset.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Loc {
    pub line: u32,
    pub col: u32,
    pub offset: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Pos {
    pub start: Loc,
    pub end: Loc,
}

impl Pos {
    pub fn range(&self) -> Range<usize> {
        self.start.offset as usize..self.end.offset as usize
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum LinkKind {
    /// `[[target#sub|alias]]`
    Wiki,
    /// `[text](target)` with a vault-relative target
    Markdown,
}

/// One reference to another note or file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LinkRef {
    pub pos: Pos,
    /// Target as written, including any `#heading` or `#^block` part.
    pub link: String,
    /// Alias or link text, when the author gave one.
    pub display: Option<String>,
    /// The exact source text, e.g. `[[Note|alias]]`.
    pub original: String,
    pub kind: LinkKind,
}

impl LinkRef {
    /// The target with any `#subpath` removed.
    pub fn path_part(&self) -> &str {
        split_subpath(&self.link).0
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Heading {
    pub pos: Pos,
    pub text: String,
    pub level: u8,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TagRef {
    pub pos: Pos,
    /// Includes the leading `#`.
    pub tag: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ListItem {
    pub pos: Pos,
    /// The character between the brackets of `- [x]`, if this is a task.
    pub task: Option<char>,
    /// Nesting depth, 0 for a top-level item.
    pub depth: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BlockId {
    /// Range of the block the id names (paragraph, list item, table...).
    pub pos: Pos,
    pub id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FrontmatterLink {
    /// Dotted key path, e.g. `related` or `project.parent`.
    pub key: String,
    pub link: String,
    pub display: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Frontmatter {
    pub pos: Pos,
    /// Parsed YAML as JSON; `Null` if the YAML failed to parse.
    pub data: serde_json::Value,
    pub links: Vec<FrontmatterLink>,
}

/// Everything extracted from one note.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct NoteMeta {
    pub links: Vec<LinkRef>,
    pub embeds: Vec<LinkRef>,
    pub tags: Vec<TagRef>,
    pub headings: Vec<Heading>,
    pub list_items: Vec<ListItem>,
    /// Keyed by lower-cased id.
    pub blocks: BTreeMap<String, BlockId>,
    pub frontmatter: Option<Frontmatter>,
}

impl NoteMeta {
    /// Body tags plus `tags:` / `tag:` frontmatter values, each with `#`.
    pub fn all_tags(&self) -> Vec<String> {
        let mut out: Vec<String> = self.tags.iter().map(|t| t.tag.clone()).collect();
        if let Some(fm) = &self.frontmatter {
            for key in ["tags", "tag"] {
                match fm.data.get(key) {
                    Some(serde_json::Value::String(s)) => {
                        for t in s.split([',', ' ']).filter(|t| !t.is_empty()) {
                            out.push(format!("#{}", t.trim_start_matches('#')));
                        }
                    }
                    Some(serde_json::Value::Array(a)) => {
                        for v in a.iter().filter_map(|v| v.as_str()) {
                            out.push(format!("#{}", v.trim_start_matches('#')));
                        }
                    }
                    _ => {}
                }
            }
        }
        out
    }

    /// Links and embeds together, in source order.
    pub fn references(&self) -> impl Iterator<Item = &LinkRef> {
        let mut all: Vec<&LinkRef> = self.links.iter().chain(self.embeds.iter()).collect();
        all.sort_by_key(|l| l.pos.start.offset);
        all.into_iter()
    }
}

/// Splits `Note#Heading` into (`Note`, `#Heading`).
pub fn split_subpath(link: &str) -> (&str, &str) {
    match link.find('#') {
        Some(i) => (&link[..i], &link[i..]),
        None => (link, ""),
    }
}

// --------------------------------------------------------------- positions

struct Lines {
    starts: Vec<usize>,
}

impl Lines {
    fn new(text: &str) -> Self {
        let mut starts = vec![0];
        for (i, b) in text.bytes().enumerate() {
            if b == b'\n' {
                starts.push(i + 1);
            }
        }
        Lines { starts }
    }

    fn loc(&self, offset: usize) -> Loc {
        let line = match self.starts.binary_search(&offset) {
            Ok(i) => i,
            Err(i) => i - 1,
        };
        Loc {
            line: line as u32,
            col: (offset - self.starts[line]) as u32,
            offset: offset as u32,
        }
    }

    fn pos(&self, r: Range<usize>) -> Pos {
        Pos {
            start: self.loc(r.start),
            end: self.loc(r.end),
        }
    }
}

// ------------------------------------------------------------------ parse

/// Parses one note. Never fails: malformed input yields partial metadata.
pub fn parse(text: &str) -> NoteMeta {
    // A locked note's links, tags and headings stay private.
    if crate::lock::is_locked(text) {
        return NoteMeta::default();
    }
    let lines = Lines::new(text);
    let mut meta = NoteMeta::default();

    let mut excluded: Vec<Range<usize>> = Vec::new();
    let mut paragraphs: Vec<Range<usize>> = Vec::new();
    let mut top_blocks: Vec<Range<usize>> = Vec::new();
    let mut item_stack: Vec<usize> = Vec::new(); // indexes into list_items
    let mut list_depth: u32 = 0;
    let mut nesting: u32 = 0; // container depth, to spot top-level blocks
    let mut heading: Option<(u8, Range<usize>)> = None;
    let mut link: Option<(String, Range<usize>, String, bool)> = None; // dest, range, text, is_image

    let opts = Options::ENABLE_TABLES
        | Options::ENABLE_TASKLISTS
        | Options::ENABLE_STRIKETHROUGH
        | Options::ENABLE_FOOTNOTES
        | Options::ENABLE_YAML_STYLE_METADATA_BLOCKS;

    for (ev, range) in Parser::new_ext(text, opts).into_offset_iter() {
        match ev {
            Event::Start(tag) => {
                if nesting == 0 {
                    top_blocks.push(range.clone());
                }
                if is_container(&tag) {
                    nesting += 1;
                }
                match tag {
                    Tag::MetadataBlock(MetadataBlockKind::YamlStyle) => {
                        excluded.push(range.clone());
                        meta.frontmatter = Some(parse_frontmatter(text, range, &lines));
                    }
                    Tag::CodeBlock(_) | Tag::HtmlBlock => excluded.push(range),
                    Tag::Heading { level, .. } => heading = Some((level as u8, range)),
                    Tag::Paragraph => paragraphs.push(range),
                    Tag::List(_) => list_depth += 1,
                    Tag::Item => {
                        let task = task_char(&text[range.clone()]);
                        item_stack.push(meta.list_items.len());
                        meta.list_items.push(ListItem {
                            pos: lines.pos(trim_trailing_newlines(text, range)),
                            task,
                            depth: list_depth.saturating_sub(1),
                        });
                    }
                    Tag::Link { dest_url, .. } => {
                        link = Some((dest_url.to_string(), range, String::new(), false))
                    }
                    Tag::Image { dest_url, .. } => {
                        link = Some((dest_url.to_string(), range, String::new(), true))
                    }
                    _ => {}
                }
            }
            Event::End(end) => {
                if is_container_end(&end) {
                    nesting = nesting.saturating_sub(1);
                }
                match end {
                    TagEnd::Heading(_) => {
                        if let Some((level, r)) = heading.take() {
                            meta.headings.push(Heading {
                                pos: lines.pos(trim_trailing_newlines(text, r.clone())),
                                text: heading_text(&text[r]),
                                level,
                            });
                        }
                    }
                    TagEnd::List(_) => list_depth = list_depth.saturating_sub(1),
                    TagEnd::Item => {
                        item_stack.pop();
                    }
                    TagEnd::Link | TagEnd::Image => {
                        if let Some((dest, r, label, is_image)) = link.take() {
                            if let Some(target) = internal_target(&dest) {
                                let lr = LinkRef {
                                    pos: lines.pos(r.clone()),
                                    link: target,
                                    display: (!label.is_empty()).then_some(label),
                                    original: text[r].to_string(),
                                    kind: LinkKind::Markdown,
                                };
                                if is_image {
                                    meta.embeds.push(lr)
                                } else {
                                    meta.links.push(lr)
                                }
                            }
                        }
                    }
                    _ => {}
                }
            }
            Event::Code(s) => {
                excluded.push(range);
                if let Some((_, _, label, _)) = link.as_mut() {
                    label.push_str(&s);
                }
            }
            Event::InlineMath(_)
            | Event::DisplayMath(_)
            | Event::InlineHtml(_)
            | Event::Html(_) => excluded.push(range),
            Event::Text(s) => {
                if let Some((_, _, label, _)) = link.as_mut() {
                    label.push_str(&s);
                }
            }
            _ => {}
        }
    }

    excluded.extend(comment_ranges(text, &excluded));
    excluded.sort_by_key(|r| r.start);

    scan_inline(text, &lines, &excluded, &mut meta);
    scan_block_ids(text, &lines, &excluded, &paragraphs, &top_blocks, &mut meta);

    meta.links.sort_by_key(|l| l.pos.start.offset);
    meta.embeds.sort_by_key(|l| l.pos.start.offset);
    meta
}

/// Block-level tags; inline formatting does not count towards nesting.
fn is_container(tag: &Tag) -> bool {
    !matches!(
        tag,
        Tag::Emphasis | Tag::Strong | Tag::Strikethrough | Tag::Link { .. } | Tag::Image { .. }
    )
}

fn is_container_end(end: &TagEnd) -> bool {
    !matches!(
        end,
        TagEnd::Emphasis | TagEnd::Strong | TagEnd::Strikethrough | TagEnd::Link | TagEnd::Image
    )
}

fn trim_trailing_newlines(text: &str, r: Range<usize>) -> Range<usize> {
    let mut end = r.end;
    while end > r.start && matches!(text.as_bytes()[end - 1], b'\n' | b'\r') {
        end -= 1;
    }
    r.start..end
}

fn heading_text(src: &str) -> String {
    let first = src.lines().next().unwrap_or("").trim();
    let s = first.trim_start_matches('#').trim();
    // Closing ATX hashes: "## Title ##"
    let s = s.trim_end_matches('#').trim_end();
    // A trailing block id is not part of the heading text.
    match s.rfind(" ^") {
        Some(i) if is_block_id(&s[i + 2..]) => s[..i].trim_end().to_string(),
        _ => s.to_string(),
    }
}

fn task_char(item_src: &str) -> Option<char> {
    let line = item_src.lines().next()?.trim_start();
    let rest = if let Some(r) = line.strip_prefix(['-', '*', '+']) {
        r
    } else {
        let digits = line.bytes().take_while(|b| b.is_ascii_digit()).count();
        if digits == 0 {
            return None;
        }
        line[digits..].strip_prefix(['.', ')'])?
    };
    let rest = rest.strip_prefix([' ', '\t'])?.trim_start();
    let mut chars = rest.chars();
    if chars.next()? != '[' {
        return None;
    }
    let c = chars.next()?;
    if chars.next()? != ']' {
        return None;
    }
    match chars.next() {
        None | Some(' ') | Some('\t') => Some(c),
        _ => None,
    }
}

/// Returns the decoded vault target for a Markdown link, or `None` for
/// external URLs and pure in-page anchors.
fn internal_target(dest: &str) -> Option<String> {
    let d = dest.trim();
    if d.is_empty() {
        return None;
    }
    if let Some(colon) = d.find(':') {
        let scheme = &d[..colon];
        if !scheme.is_empty()
            && scheme
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || "+-.".contains(c))
        {
            return None; // http:, mailto:, obsidian:, file: ...
        }
    }
    Some(percent_decode(d))
}

fn percent_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' && i + 2 < b.len() {
            if let Ok(v) = u8::from_str_radix(&s[i + 1..i + 3], 16) {
                out.push(v);
                i += 3;
                continue;
            }
        }
        out.push(b[i]);
        i += 1;
    }
    String::from_utf8(out).unwrap_or_else(|_| s.to_string())
}

fn parse_frontmatter(text: &str, range: Range<usize>, lines: &Lines) -> Frontmatter {
    let src = &text[range.clone()];
    // Strip the opening and closing fence lines.
    let body: String = src
        .lines()
        .skip(1)
        .take_while(|l| l.trim_end() != "---" && l.trim_end() != "...")
        .collect::<Vec<_>>()
        .join("\n");
    let data: serde_json::Value = serde_yaml_ng::from_str(&body).unwrap_or(serde_json::Value::Null);
    let data = if data.is_null() && body.trim().is_empty() {
        serde_json::Value::Object(Default::default())
    } else {
        data
    };
    let mut links = Vec::new();
    collect_fm_links(&data, String::new(), &mut links);
    Frontmatter {
        pos: lines.pos(trim_trailing_newlines(text, range)),
        data,
        links,
    }
}

fn collect_fm_links(v: &serde_json::Value, key: String, out: &mut Vec<FrontmatterLink>) {
    match v {
        serde_json::Value::String(s) => {
            let t = s.trim();
            if let Some(inner) = t.strip_prefix("[[").and_then(|r| r.strip_suffix("]]")) {
                if !inner.contains("[[") && !inner.contains("]]") {
                    let (link, display) = split_alias(inner);
                    if !link.is_empty() {
                        out.push(FrontmatterLink { key, link, display });
                    }
                }
            }
        }
        serde_json::Value::Array(a) => {
            for (i, item) in a.iter().enumerate() {
                let k = if key.is_empty() {
                    i.to_string()
                } else {
                    format!("{key}.{i}")
                };
                collect_fm_links(item, k, out);
            }
        }
        serde_json::Value::Object(o) => {
            for (k, item) in o {
                let k = if key.is_empty() {
                    k.clone()
                } else {
                    format!("{key}.{k}")
                };
                collect_fm_links(item, k, out);
            }
        }
        _ => {}
    }
}

/// `Note#H|alias` → (`Note#H`, Some(`alias`)). A `\|` (used inside tables)
/// also splits, and its backslash is dropped.
fn split_alias(inner: &str) -> (String, Option<String>) {
    match inner.find('|') {
        Some(i) => {
            let mut target = &inner[..i];
            if let Some(t) = target.strip_suffix('\\') {
                target = t;
            }
            let alias = inner[i + 1..].trim();
            (
                target.trim().to_string(),
                (!alias.is_empty()).then(|| alias.to_string()),
            )
        }
        None => (inner.trim().to_string(), None),
    }
}

/// `%% ... %%` comment ranges outside code.
fn comment_ranges(text: &str, code: &[Range<usize>]) -> Vec<Range<usize>> {
    let mut out = Vec::new();
    let mut open: Option<usize> = None;
    let mut i = 0;
    let b = text.as_bytes();
    while i + 1 < b.len() {
        if b[i] == b'%' && b[i + 1] == b'%' && !inside(code, i) {
            match open.take() {
                None => open = Some(i),
                Some(s) => out.push(s..i + 2),
            }
            i += 2;
            continue;
        }
        i += 1;
    }
    if let Some(s) = open {
        out.push(s..text.len()); // unterminated comment runs to the end
    }
    out
}

fn inside(ranges: &[Range<usize>], i: usize) -> bool {
    ranges.iter().any(|r| r.contains(&i))
}

/// Next excluded range at or after `i`, given ranges sorted by start.
fn skip_excluded(ranges: &[Range<usize>], i: usize) -> Option<usize> {
    ranges.iter().find(|r| r.contains(&i)).map(|r| r.end)
}

fn scan_inline(text: &str, lines: &Lines, excluded: &[Range<usize>], meta: &mut NoteMeta) {
    let b = text.as_bytes();
    let mut i = 0;
    while i < b.len() {
        if let Some(end) = skip_excluded(excluded, i) {
            i = end.max(i + 1);
            continue;
        }
        match b[i] {
            b'\\' => {
                i += 2; // escaped character
                continue;
            }
            b'[' if b.get(i + 1) == Some(&b'[') => {
                if let Some(close) = find_wikilink_end(text, i + 2) {
                    let inner = &text[i + 2..close];
                    let embed = i > 0 && b[i - 1] == b'!' && !(i > 1 && b[i - 2] == b'\\');
                    let start = if embed { i - 1 } else { i };
                    let (link, display) = split_alias(inner);
                    let r = start..close + 2;
                    let lr = LinkRef {
                        pos: lines.pos(r.clone()),
                        link,
                        display,
                        original: text[r].to_string(),
                        kind: LinkKind::Wiki,
                    };
                    if embed {
                        meta.embeds.push(lr)
                    } else {
                        meta.links.push(lr)
                    }
                    i = close + 2;
                    continue;
                }
            }
            b'#' => {
                let prev_ok = i == 0 || {
                    let p = prev_char(text, i);
                    p.is_whitespace() || matches!(p, '(' | '[' | ',' | ';')
                };
                if prev_ok {
                    let len = tag_len(&text[i + 1..]);
                    if len > 0 {
                        let r = i..i + 1 + len;
                        meta.tags.push(TagRef {
                            pos: lines.pos(r.clone()),
                            tag: text[r].to_string(),
                        });
                        i += 1 + len;
                        continue;
                    }
                }
            }
            _ => {}
        }
        i += 1;
    }
}

fn prev_char(text: &str, i: usize) -> char {
    text[..i].chars().next_back().unwrap_or(' ')
}

/// Finds the `]]` that closes a wikilink opened before `from`, on the same
/// line, with no nested `[[`.
fn find_wikilink_end(text: &str, from: usize) -> Option<usize> {
    let rest = &text[from..];
    let line_end = rest.find('\n').unwrap_or(rest.len());
    let line = &rest[..line_end];
    let close = line.find("]]")?;
    if close == 0 || line[..close].contains("[[") {
        return None;
    }
    Some(from + close)
}

/// Length in bytes of a tag body starting right after `#`, or 0 if none.
/// Tags may contain letters, digits, `_`, `-`, `/` and any non-ASCII
/// character, and must not be all digits.
fn tag_len(s: &str) -> usize {
    let mut len = 0;
    let mut has_non_digit = false;
    for c in s.chars() {
        let ok = c.is_alphanumeric()
            || matches!(c, '_' | '-' | '/')
            || (!c.is_ascii() && !c.is_whitespace() && !is_unicode_punct(c));
        if !ok {
            break;
        }
        if !c.is_ascii_digit() {
            has_non_digit = true;
        }
        len += c.len_utf8();
    }
    if has_non_digit {
        len
    } else {
        0
    }
}

fn is_unicode_punct(c: char) -> bool {
    matches!(c, '\u{2000}'..='\u{206F}' | '\u{3000}'..='\u{303F}' | '\u{FF01}'..='\u{FF0F}')
}

fn is_block_id(s: &str) -> bool {
    !s.is_empty() && s.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
}

/// `text ^id` at the end of a paragraph or list item line names that block.
/// `^id` alone on a line names the block just above it (tables, quotes).
fn scan_block_ids(
    text: &str,
    lines: &Lines,
    excluded: &[Range<usize>],
    paragraphs: &[Range<usize>],
    top_blocks: &[Range<usize>],
    meta: &mut NoteMeta,
) {
    let mut offset = 0;
    for line in text.split_inclusive('\n') {
        let start = offset;
        offset += line.len();
        let content = line.trim_end_matches(['\n', '\r']).trim_end();
        let Some(caret) = content.rfind('^') else {
            continue;
        };
        let id = &content[caret + 1..];
        if !is_block_id(id) || inside(excluded, start + caret) {
            continue;
        }
        let before = &content[..caret];
        let alone = before.trim().is_empty();
        if !alone && !before.ends_with([' ', '\t']) {
            continue;
        }
        let at = start + caret;
        let target: Option<Range<usize>> = if alone {
            top_blocks
                .iter()
                .filter(|r| r.end <= start + 1 && r.start < start)
                .max_by_key(|r| r.start)
                .cloned()
        } else {
            // Innermost list item or paragraph containing the id.
            let item = meta
                .list_items
                .iter()
                .filter(|li| li.pos.range().contains(&at))
                .max_by_key(|li| li.pos.start.offset)
                .map(|li| li.pos.range());
            item.or_else(|| {
                paragraphs
                    .iter()
                    .filter(|r| r.contains(&at))
                    .max_by_key(|r| r.start)
                    .cloned()
            })
            .or_else(|| {
                meta.headings
                    .iter()
                    .find(|h| h.pos.range().contains(&at))
                    .map(|h| h.pos.range())
            })
        };
        if let Some(r) = target {
            let r = trim_trailing_newlines(text, r);
            meta.blocks.entry(id.to_lowercase()).or_insert(BlockId {
                pos: lines.pos(r),
                id: id.to_string(),
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn links(m: &NoteMeta) -> Vec<(&str, Option<&str>)> {
        m.links
            .iter()
            .map(|l| (l.link.as_str(), l.display.as_deref()))
            .collect()
    }

    #[test]
    fn wikilinks_aliases_and_embeds() {
        let m = parse("See [[Note]], [[Other#Heading|shown]] and ![[img.png]].\n");
        assert_eq!(
            links(&m),
            vec![("Note", None), ("Other#Heading", Some("shown"))]
        );
        assert_eq!(m.embeds.len(), 1);
        assert_eq!(m.embeds[0].link, "img.png");
        assert_eq!(m.embeds[0].original, "![[img.png]]");
        assert_eq!(m.links[0].pos.start.col, 4);
        assert_eq!(m.links[1].path_part(), "Other");
    }

    #[test]
    fn ignores_links_in_code_and_comments() {
        let src = "`[[inline]]`\n\n```\n[[fenced]] #nottag\n```\n\n%% [[hidden]] %%\n\n[[real]]\n";
        let m = parse(src);
        assert_eq!(links(&m), vec![("real", None)]);
        assert!(m.tags.is_empty());
    }

    #[test]
    fn table_escaped_pipe() {
        let m = parse("| a | b |\n|---|---|\n| [[Note\\|alias]] | x |\n");
        assert_eq!(links(&m), vec![("Note", Some("alias"))]);
    }

    #[test]
    fn markdown_links_internal_only() {
        let m = parse(
            "[x](Folder/My%20Note.md) [web](https://a.com) [mail](mailto:a@b.c) ![p](pic.png)\n",
        );
        assert_eq!(links(&m), vec![("Folder/My Note.md", Some("x"))]);
        assert_eq!(m.embeds[0].link, "pic.png");
        assert_eq!(m.links[0].kind, LinkKind::Markdown);
    }

    #[test]
    fn tags() {
        let m = parse("# Heading\n#tag and #nested/tag, #123 no, a#b no, #über ok\n");
        let t: Vec<_> = m.tags.iter().map(|t| t.tag.as_str()).collect();
        assert_eq!(t, vec!["#tag", "#nested/tag", "#über"]);
    }

    #[test]
    fn headings_with_levels() {
        let m = parse("# One\n\ntext\n\n## Two ##\n\nSetext\n---\n");
        let h: Vec<_> = m
            .headings
            .iter()
            .map(|h| (h.text.as_str(), h.level))
            .collect();
        assert_eq!(h, vec![("One", 1), ("Two", 2), ("Setext", 2)]);
        assert_eq!(m.headings[1].pos.start.line, 4);
    }

    #[test]
    fn tasks_and_nesting() {
        let m = parse("- [ ] todo\n- [x] done\n    - [/] half\n- plain\n1. [-] cancelled\n");
        let t: Vec<_> = m.list_items.iter().map(|l| (l.task, l.depth)).collect();
        assert_eq!(
            t,
            vec![
                (Some(' '), 0),
                (Some('x'), 0),
                (Some('/'), 1),
                (None, 0),
                (Some('-'), 0)
            ]
        );
    }

    #[test]
    fn block_ids() {
        let src =
            "First para ^p1\n\n- item ^Item-2\n\n| a |\n|---|\n| b |\n\n^tbl\n\nnot an id^x\n";
        let m = parse(src);
        let ids: Vec<_> = m.blocks.keys().cloned().collect();
        assert_eq!(ids, vec!["item-2", "p1", "tbl"]);
        assert_eq!(m.blocks["p1"].pos.start.line, 0);
        assert_eq!(m.blocks["item-2"].id, "Item-2");
        assert_eq!(m.blocks["tbl"].pos.start.line, 4);
    }

    #[test]
    fn frontmatter() {
        let src = "---\ntitle: Hi\ntags: [a, '#b']\nrelated: \"[[Other|o]]\"\nlist:\n  - \"[[X]]\"\n---\nBody [[Y]]\n";
        let m = parse(src);
        let fm = m.frontmatter.as_ref().unwrap();
        assert_eq!(fm.data["title"], "Hi");
        assert_eq!(fm.pos.start.line, 0);
        assert_eq!(fm.pos.end.line, 6);
        let fl: Vec<_> = fm
            .links
            .iter()
            .map(|l| (l.key.as_str(), l.link.as_str()))
            .collect();
        assert_eq!(fl, vec![("list.0", "X"), ("related", "Other")]);
        assert_eq!(links(&m), vec![("Y", None)]);
        assert_eq!(m.all_tags(), vec!["#a", "#b"]);
    }

    #[test]
    fn bad_yaml_still_parses_body() {
        let m = parse("---\n: : bad\n---\n[[Ok]]\n");
        assert!(m.frontmatter.is_some());
        assert_eq!(links(&m), vec![("Ok", None)]);
    }

    #[test]
    fn crlf_positions() {
        let m = parse("line\r\n[[A]]\r\n");
        assert_eq!(m.links[0].pos.start.line, 1);
        assert_eq!(m.links[0].pos.start.col, 0);
    }
}
