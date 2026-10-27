//! Markdown → blocks and styled runs that the apps draw with their own text
//! views (SwiftUI `Text`, Compose `AnnotatedString`), so reading a note
//! looks native on each platform and the same on both.
//!
//! Also the editor's colouring: [`highlight`] marks headings, bold, links,
//! tags and so on in the raw text, in UTF-16 units (what Swift's NSString
//! and Kotlin's String count).

use std::collections::HashMap;

use pulldown_cmark::{CodeBlockKind, Event, Options, Parser, Tag, TagEnd};
use serde_json::Value;

use crate::{vp, Marrow, Result};

#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum BlockKind {
    Heading,
    Paragraph,
    /// A list item; `level` is how deep it's nested (0 = top).
    Bullet,
    /// A numbered item; `number` is its number.
    Numbered,
    /// A to-do; `checked` says whether it's done.
    Task,
    Code,
    Table,
    Rule,
    /// An image (or `![[embed]]` of one): `target`.
    Image,
    /// An embedded note, PDF, recording or other file: `target`.
    Embed,
    /// The note's properties (frontmatter).
    Properties,
    /// The title line of a callout (`> [!tip] Title`); `callout` is its kind.
    CalloutTitle,
    /// Display math (`$$ … $$`), shown as `text`.
    Math,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum LinkKind {
    Url,
    /// A note (or file) in the vault; `target` is its path.
    Note,
    /// A note that doesn't exist yet; `target` is the link text.
    Missing,
    /// A `#tag`; `target` is the tag with `#`.
    Tag,
}

#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct SpanLink {
    pub kind: LinkKind,
    pub target: String,
}

/// A run of text with one style.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct Span {
    pub text: String,
    pub bold: bool,
    pub italic: bool,
    pub strike: bool,
    pub code: bool,
    pub highlight: bool,
    pub link: Option<SpanLink>,
}

#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct TableRow {
    pub header: bool,
    pub cells: Vec<Vec<Span>>,
}

#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct Property {
    pub key: String,
    pub value: String,
}

#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct Block {
    pub kind: BlockKind,
    pub spans: Vec<Span>,
    /// Zero-based line in the note where it starts (to edit or tick it).
    pub line: u32,
    /// Heading level (1–6), or list nesting (0 = top).
    pub level: u8,
    /// How many quotes (or callouts) it sits in.
    pub quote: u8,
    /// The kind of callout it sits in ("tip", "warning"…), or "".
    pub callout: String,
    pub number: u32,
    pub checked: bool,
    /// Code, math, or an image's alt text.
    pub text: String,
    /// A code block's language.
    pub language: String,
    pub rows: Vec<TableRow>,
    /// For images and embeds: a vault path, or a web address.
    pub target: String,
    /// The target is a file in the vault (else a web address or missing).
    pub local: bool,
    pub props: Vec<Property>,
}

impl Block {
    fn new(kind: BlockKind, line: u32) -> Self {
        Block {
            kind,
            spans: vec![],
            line,
            level: 0,
            quote: 0,
            callout: String::new(),
            number: 0,
            checked: false,
            text: String::new(),
            language: String::new(),
            rows: vec![],
            target: String::new(),
            local: false,
            props: vec![],
        }
    }
}

#[derive(Clone, Copy, Default, PartialEq)]
struct Style {
    bold: bool,
    italic: bool,
    strike: bool,
}

/// Where the line starts are, to turn byte offsets into line numbers.
struct Lines(Vec<usize>);

impl Lines {
    fn new(text: &str) -> Self {
        let mut v = vec![0];
        v.extend(text.match_indices('\n').map(|(i, _)| i + 1));
        Lines(v)
    }
    fn line(&self, offset: usize) -> u32 {
        (self.0.partition_point(|&s| s <= offset).saturating_sub(1)) as u32
    }
}

/// How links in a note are looked up while rendering.
type Resolver<'a> = &'a dyn Fn(&str) -> Option<String>;

struct Builder<'a> {
    blocks: Vec<Block>,
    /// The block being filled, if any.
    cur: Option<Block>,
    /// Inline text waiting to be split into wikilinks, tags and highlights.
    pending: String,
    pending_style: Style,
    style: Style,
    link: Option<SpanLink>,
    /// Inside an image: its target and alt text so far.
    image: Option<(String, String)>,
    quote: u8,
    callout: Vec<String>,
    /// A quote's first paragraph may be a callout title.
    quote_fresh: bool,
    lists: Vec<Option<u64>>,
    /// Item number for the next item in each open list.
    item_open: bool,
    code: Option<Block>,
    table: Option<(Vec<TableRow>, bool)>,
    cell: Option<Vec<Span>>,
    highlight: bool,
    resolve: Resolver<'a>,
    lines: Lines,
}

fn is_image_name(s: &str) -> bool {
    let l = s.to_ascii_lowercase();
    [".png", ".jpg", ".jpeg", ".gif", ".webp", ".svg", ".bmp", ".heic", ".heif", ".avif"].iter().any(|e| l.ends_with(e))
}

fn is_web(s: &str) -> bool {
    let l = s.to_ascii_lowercase();
    l.starts_with("http://") || l.starts_with("https://") || l.starts_with("mailto:")
}

impl<'a> Builder<'a> {
    fn begin(&mut self, kind: BlockKind, offset: usize) {
        self.flush_block();
        let mut b = Block::new(kind, self.lines.line(offset));
        b.quote = self.quote;
        b.callout = self.callout.last().cloned().unwrap_or_default();
        b.level = self.lists.len().saturating_sub(1) as u8;
        self.cur = Some(b);
    }

    fn flush_text(&mut self) {
        if self.pending.is_empty() {
            return;
        }
        let text = std::mem::take(&mut self.pending);
        let style = self.pending_style;
        let link = self.link.clone();
        let spans = self.split_inline(&text, style, link);
        self.push_spans(spans);
    }

    fn push_spans(&mut self, spans: Vec<Span>) {
        if let Some(cell) = self.cell.as_mut() {
            cell.extend(spans);
            return;
        }
        if self.cur.is_none() {
            let line = 0;
            let mut b = Block::new(BlockKind::Paragraph, line);
            b.quote = self.quote;
            b.callout = self.callout.last().cloned().unwrap_or_default();
            b.level = self.lists.len() as u8;
            self.cur = Some(b);
        }
        let cur = self.cur.as_mut().unwrap();
        for s in spans {
            match cur.spans.last_mut() {
                Some(last) if last.bold == s.bold && last.italic == s.italic && last.strike == s.strike && last.code == s.code && last.highlight == s.highlight && last.link == s.link => {
                    last.text.push_str(&s.text)
                }
                _ => cur.spans.push(s),
            }
        }
    }

    fn text(&mut self, t: &str) {
        if self.image.is_some() {
            if let Some((_, alt)) = self.image.as_mut() {
                alt.push_str(t);
            }
            return;
        }
        if self.pending_style != self.style && !self.pending.is_empty() {
            self.flush_text();
        }
        self.pending_style = self.style;
        self.pending.push_str(t);
    }

    fn span(&self, text: &str, style: Style) -> Span {
        Span { text: text.to_string(), bold: style.bold, italic: style.italic, strike: style.strike, code: false, highlight: self.highlight, link: None }
    }

    /// Splits text into plain runs, `[[wikilinks]]`, `![[embeds]]`, `#tags` and `==highlights==`.
    fn split_inline(&mut self, text: &str, style: Style, link: Option<SpanLink>) -> Vec<Span> {
        let mut out = Vec::new();
        let mut plain = String::new();
        let b = text.as_bytes();
        let mut i = 0;
        let flush = |plain: &mut String, out: &mut Vec<Span>, me: &Self| {
            if !plain.is_empty() {
                let mut s = me.span(plain, style);
                s.link = link.clone();
                out.push(s);
                plain.clear();
            }
        };
        while i < b.len() {
            if link.is_none() && b[i] == b'[' && b.get(i + 1) == Some(&b'[') {
                if let Some(close) = text[i + 2..].find("]]").map(|c| i + 2 + c) {
                    let inner = &text[i + 2..close];
                    if !inner.is_empty() && !inner.contains('\n') && !inner.contains("[[") {
                        let embed = i > 0 && b[i - 1] == b'!';
                        if embed {
                            plain.pop();
                        }
                        flush(&mut plain, &mut out, self);
                        let (target, alias) = match inner.split_once('|') {
                            Some((t, a)) => (t.trim(), Some(a.trim())),
                            None => (inner.trim(), None),
                        };
                        if embed {
                            let before = std::mem::take(&mut out);
                            self.push_spans(before);
                            self.embed(target, alias.unwrap_or(""));
                        } else {
                            let shown = alias.map(str::to_string).unwrap_or_else(|| {
                                let (note, sub) = marrow_core::parse::split_subpath(target);
                                let base = note.rsplit('/').next().unwrap_or(note);
                                let base = base.strip_suffix(".md").unwrap_or(base);
                                if sub.is_empty() { base.to_string() } else if base.is_empty() { sub.trim_start_matches('#').to_string() } else { format!("{base} › {}", sub.trim_start_matches('#').trim_start_matches('^')) }
                            });
                            let mut s = self.span(&shown, style);
                            s.link = Some(match (self.resolve)(target) {
                                Some(p) => SpanLink { kind: LinkKind::Note, target: p },
                                None => SpanLink { kind: LinkKind::Missing, target: target.to_string() },
                            });
                            out.push(s);
                        }
                        i = close + 2;
                        continue;
                    }
                }
            }
            if b[i] == b'=' && b.get(i + 1) == Some(&b'=') {
                flush(&mut plain, &mut out, self);
                self.highlight = !self.highlight;
                i += 2;
                continue;
            }
            if link.is_none() && b[i] == b'#' {
                let prev_ok = i == 0 || {
                    let p = text[..i].chars().next_back().unwrap_or(' ');
                    p.is_whitespace() || matches!(p, '(' | '[' | ',' | ';')
                };
                let len = if prev_ok { marrow_core::parse::tag_len(&text[i + 1..]) } else { 0 };
                if len > 0 {
                    flush(&mut plain, &mut out, self);
                    let tag = &text[i..i + 1 + len];
                    let mut s = self.span(tag, style);
                    s.link = Some(SpanLink { kind: LinkKind::Tag, target: tag.to_string() });
                    out.push(s);
                    i += 1 + len;
                    continue;
                }
            }
            let c = text[i..].chars().next().unwrap();
            plain.push(c);
            i += c.len_utf8();
        }
        flush(&mut plain, &mut out, self);
        out
    }

    /// An image or embedded file, as its own block.
    fn embed(&mut self, target: &str, alt: &str) {
        let (file, _) = marrow_core::parse::split_subpath(target);
        let mut b = Block::new(if is_image_name(file) || (is_web(file) && is_image_name(file.split('?').next().unwrap_or(file))) { BlockKind::Image } else { BlockKind::Embed }, self.cur.as_ref().map(|c| c.line).unwrap_or(0));
        b.quote = self.quote;
        b.callout = self.callout.last().cloned().unwrap_or_default();
        b.text = alt.to_string();
        if is_web(file) {
            b.target = file.to_string();
            if b.kind == BlockKind::Embed {
                b.kind = BlockKind::Image;
            }
        } else if let Some(p) = (self.resolve)(file) {
            b.target = p;
            b.local = true;
        } else {
            b.target = file.to_string();
        }
        if b.kind == BlockKind::Embed && b.text.is_empty() {
            b.text = crate::files::display_name(&b.target);
        }
        // Text before the embed stays its own block; text after continues a new one.
        let line = b.line;
        let kind = self.cur.as_ref().map(|c| c.kind);
        let had_text = self.cur.as_ref().is_some_and(|c| c.spans.iter().any(|s| !s.text.trim().is_empty()));
        if had_text {
            self.flush_block();
        } else if kind.is_some() && kind != Some(BlockKind::Paragraph) {
            // Keep a list item or heading even when the embed is all it has.
            self.flush_block();
        } else {
            self.cur = None;
        }
        self.blocks.push(b);
        let mut next = Block::new(BlockKind::Paragraph, line);
        next.quote = self.quote;
        next.callout = self.callout.last().cloned().unwrap_or_default();
        self.cur = Some(next);
    }

    fn flush_block(&mut self) {
        self.flush_text();
        if let Some(mut b) = self.cur.take() {
            // Trim line breaks at the ends.
            while b.spans.first().is_some_and(|s| s.text.trim().is_empty() && s.link.is_none()) {
                b.spans.remove(0);
            }
            while b.spans.last().is_some_and(|s| s.text.trim().is_empty() && s.link.is_none()) {
                b.spans.pop();
            }
            if let Some(f) = b.spans.first_mut() {
                f.text = f.text.trim_start().to_string();
            }
            if let Some(l) = b.spans.last_mut() {
                l.text = l.text.trim_end().to_string();
            }
            let empty = b.spans.is_empty();
            if !empty || matches!(b.kind, BlockKind::Task | BlockKind::Bullet | BlockKind::Numbered | BlockKind::Heading) {
                self.blocks.push(b);
            }
        }
    }

    /// A quote's first paragraph `[!kind] Title` turns the quote into a callout.
    fn callout_title(&mut self) {
        let Some(cur) = self.cur.as_mut() else { return };
        let joined: String = cur.spans.iter().map(|s| s.text.as_str()).collect();
        let t = joined.trim_start();
        if !t.starts_with("[!") {
            return;
        }
        let Some(end) = t.find(']') else { return };
        let kind = t[2..end].trim().to_lowercase();
        if kind.is_empty() || kind.contains(' ') {
            return;
        }
        let after = t[end + 1..].trim_start_matches(['+', '-']);
        let (title, body) = match after.split_once('\n') {
            Some((a, b)) => (a.trim().to_string(), b.to_string()),
            None => (after.trim().to_string(), String::new()),
        };
        let title = if title.is_empty() { capitalize(&kind) } else { title };
        let line = cur.line;
        // Rebuild: drop the title line from the spans (they were plain text there).
        let mut rest: Vec<Span> = Vec::new();
        let mut skipped = 0usize;
        let skip = joined.len() - body.len();
        for s in cur.spans.drain(..) {
            if skipped >= skip {
                rest.push(s);
                continue;
            }
            let take = (skip - skipped).min(s.text.len());
            skipped += take;
            if take < s.text.len() {
                let mut s2 = s.clone();
                s2.text = s.text[take..].to_string();
                rest.push(s2);
            }
        }
        cur.spans = rest;
        if let Some(c) = self.callout.last_mut() {
            *c = kind.clone();
        }
        cur.callout = kind.clone();
        let mut tb = Block::new(BlockKind::CalloutTitle, line);
        tb.quote = self.quote;
        tb.callout = kind;
        tb.spans = vec![Span { text: title, bold: true, italic: false, strike: false, code: false, highlight: false, link: None }];
        self.blocks.push(tb);
    }
}

fn capitalize(s: &str) -> String {
    let mut c = s.chars();
    match c.next() {
        Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
        None => String::new(),
    }
}

/// A property value as words.
fn value_words(v: &Value) -> String {
    match v {
        Value::Null => String::new(),
        Value::String(s) => s.clone(),
        Value::Array(a) => a.iter().map(value_words).filter(|s| !s.is_empty()).collect::<Vec<_>>().join(", "),
        Value::Object(_) => v.to_string(),
        other => other.to_string(),
    }
}

/// The blocks of a note. `resolve` finds the vault path a link points to.
pub(crate) fn render_with(text: &str, resolve: Resolver) -> Vec<Block> {
    let opts = Options::ENABLE_TABLES | Options::ENABLE_TASKLISTS | Options::ENABLE_STRIKETHROUGH | Options::ENABLE_YAML_STYLE_METADATA_BLOCKS | Options::ENABLE_MATH;
    // %% comments %% aren't shown.
    let shown = strip_comments(text);
    let mut b = Builder {
        blocks: vec![],
        cur: None,
        pending: String::new(),
        pending_style: Style::default(),
        style: Style::default(),
        link: None,
        image: None,
        quote: 0,
        callout: vec![],
        quote_fresh: false,
        lists: vec![],
        item_open: false,
        code: None,
        table: None,
        cell: None,
        highlight: false,
        resolve,
        lines: Lines::new(&shown),
    };
    let mut in_meta = false;
    for (ev, range) in Parser::new_ext(&shown, opts).into_offset_iter() {
        match ev {
            Event::Start(tag) => match tag {
                Tag::MetadataBlock(_) => in_meta = true,
                Tag::Heading { level, .. } => {
                    b.begin(BlockKind::Heading, range.start);
                    b.cur.as_mut().unwrap().level = level as u8;
                }
                Tag::Paragraph => {
                    // The first paragraph of a list item is the item's own text.
                    if b.item_open {
                        b.item_open = false;
                    } else {
                        b.begin(BlockKind::Paragraph, range.start);
                        if !b.lists.is_empty() {
                            b.cur.as_mut().unwrap().level = b.lists.len() as u8;
                        }
                    }
                }
                Tag::BlockQuote(_) => {
                    b.flush_block();
                    b.quote += 1;
                    b.callout.push(b.callout.last().cloned().unwrap_or_default());
                    b.quote_fresh = true;
                }
                Tag::CodeBlock(kind) => {
                    b.flush_block();
                    let mut c = Block::new(BlockKind::Code, b.lines.line(range.start));
                    c.quote = b.quote;
                    c.callout = b.callout.last().cloned().unwrap_or_default();
                    c.level = b.lists.len() as u8;
                    if let CodeBlockKind::Fenced(lang) = kind {
                        c.language = lang.split_whitespace().next().unwrap_or("").to_string();
                    }
                    b.code = Some(c);
                }
                Tag::List(start) => {
                    b.flush_block();
                    b.lists.push(start);
                }
                Tag::Item => {
                    let ordered = b.lists.last().copied().flatten();
                    b.begin(if ordered.is_some() { BlockKind::Numbered } else { BlockKind::Bullet }, range.start);
                    if let Some(n) = ordered {
                        b.cur.as_mut().unwrap().number = n as u32;
                        if let Some(Some(next)) = b.lists.last_mut() {
                            *next += 1;
                        }
                    }
                    b.item_open = true;
                }
                Tag::Table(_) => {
                    b.flush_block();
                    b.table = Some((vec![], false));
                }
                Tag::TableHead => {
                    if let Some(t) = b.table.as_mut() {
                        t.1 = true;
                        t.0.push(TableRow { header: true, cells: vec![] });
                    }
                }
                Tag::TableRow => {
                    if let Some(t) = b.table.as_mut() {
                        t.0.push(TableRow { header: false, cells: vec![] });
                    }
                }
                Tag::TableCell => {
                    b.flush_text();
                    b.cell = Some(vec![]);
                }
                Tag::Emphasis => {
                    b.flush_text();
                    b.style.italic = true;
                }
                Tag::Strong => {
                    b.flush_text();
                    b.style.bold = true;
                }
                Tag::Strikethrough => {
                    b.flush_text();
                    b.style.strike = true;
                }
                Tag::Link { dest_url, .. } => {
                    b.flush_text();
                    let d = dest_url.to_string();
                    b.link = Some(if is_web(&d) {
                        SpanLink { kind: LinkKind::Url, target: d }
                    } else {
                        let decoded = marrow_webdav::percent_decode(&d);
                        match (b.resolve)(&decoded) {
                            Some(p) => SpanLink { kind: LinkKind::Note, target: p },
                            None => SpanLink { kind: LinkKind::Missing, target: decoded },
                        }
                    });
                }
                Tag::Image { dest_url, .. } => {
                    b.flush_text();
                    b.image = Some((dest_url.to_string(), String::new()));
                }
                _ => {}
            },
            Event::End(end) => match end {
                TagEnd::MetadataBlock(_) => in_meta = false,
                TagEnd::Heading(_) => b.flush_block(),
                TagEnd::Paragraph => {
                    b.flush_text();
                    if b.quote_fresh {
                        b.quote_fresh = false;
                        b.callout_title();
                    }
                    b.flush_block();
                }
                TagEnd::BlockQuote(_) => {
                    b.flush_block();
                    b.quote = b.quote.saturating_sub(1);
                    b.callout.pop();
                    b.quote_fresh = false;
                }
                TagEnd::CodeBlock => {
                    if let Some(mut c) = b.code.take() {
                        if c.text.ends_with('\n') {
                            c.text.pop();
                        }
                        if c.language == "math" {
                            c.kind = BlockKind::Math;
                        }
                        b.blocks.push(c);
                    }
                }
                TagEnd::List(_) => {
                    b.flush_block();
                    b.lists.pop();
                }
                TagEnd::Item => {
                    b.item_open = false;
                    b.flush_block();
                }
                TagEnd::Table => {
                    if let Some((rows, _)) = b.table.take() {
                        let mut t = Block::new(BlockKind::Table, b.lines.line(range.start));
                        t.quote = b.quote;
                        t.rows = rows;
                        b.blocks.push(t);
                    }
                }
                TagEnd::TableCell => {
                    b.flush_text();
                    if let (Some(cell), Some((rows, _))) = (b.cell.take(), b.table.as_mut()) {
                        if let Some(r) = rows.last_mut() {
                            r.cells.push(cell);
                        }
                    }
                }
                TagEnd::Emphasis => {
                    b.flush_text();
                    b.style.italic = false;
                }
                TagEnd::Strong => {
                    b.flush_text();
                    b.style.bold = false;
                }
                TagEnd::Strikethrough => {
                    b.flush_text();
                    b.style.strike = false;
                }
                TagEnd::Link => {
                    b.flush_text();
                    b.link = None;
                }
                TagEnd::Image => {
                    if let Some((dest, alt)) = b.image.take() {
                        let d = if is_web(&dest) { dest } else { marrow_webdav::percent_decode(&dest) };
                        b.embed(&d, &alt);
                    }
                }
                _ => {}
            },
            Event::Text(t) => {
                if in_meta {
                    continue;
                }
                if let Some(c) = b.code.as_mut() {
                    c.text.push_str(&t);
                } else {
                    b.text(&t);
                }
            }
            Event::Code(t) => {
                b.flush_text();
                let mut s = b.span(&t, b.style);
                s.code = true;
                s.link = b.link.clone();
                b.push_spans(vec![s]);
            }
            Event::InlineMath(t) => {
                b.flush_text();
                let mut s = b.span(&t, b.style);
                s.italic = true;
                b.push_spans(vec![s]);
            }
            Event::DisplayMath(t) => {
                b.flush_block();
                let mut m = Block::new(BlockKind::Math, b.lines.line(range.start));
                m.text = t.trim().to_string();
                m.quote = b.quote;
                b.blocks.push(m);
                b.cur = None;
            }
            Event::SoftBreak | Event::HardBreak => b.text("\n"),
            Event::Rule => {
                b.flush_block();
                let r = Block::new(BlockKind::Rule, b.lines.line(range.start));
                b.blocks.push(r);
            }
            Event::TaskListMarker(done) => {
                if let Some(c) = b.cur.as_mut() {
                    c.kind = BlockKind::Task;
                    c.checked = done;
                }
            }
            Event::Html(h) | Event::InlineHtml(h) => {
                // Shown as written, except comments and line breaks.
                let t = h.trim();
                if t.eq_ignore_ascii_case("<br>") || t.eq_ignore_ascii_case("<br/>") || t.eq_ignore_ascii_case("<br />") {
                    b.text("\n");
                } else if !t.starts_with("<!--") {
                    b.text(&h);
                }
            }
            Event::FootnoteReference(f) => b.text(&format!("[{f}]")),
        }
    }
    b.flush_block();
    // Properties go first.
    let meta = marrow_core::parse::parse(text);
    if let Some(fm) = meta.frontmatter {
        if let Value::Object(map) = fm.data {
            let props: Vec<Property> = map.iter().map(|(k, v)| Property { key: k.clone(), value: value_words(v) }).collect();
            if !props.is_empty() {
                let mut p = Block::new(BlockKind::Properties, 0);
                p.props = props;
                b.blocks.insert(0, p);
            }
        }
    }
    b.blocks
}

/// Blanks out `%% comments %%`, keeping line numbers.
fn strip_comments(text: &str) -> String {
    if !text.contains("%%") {
        return text.to_string();
    }
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(i) = rest.find("%%") {
        out.push_str(&rest[..i]);
        let after = &rest[i + 2..];
        match after.find("%%") {
            Some(j) => {
                out.extend(after[..j].chars().filter(|c| *c == '\n'));
                rest = &after[j + 2..];
            }
            None => {
                rest = "";
            }
        }
    }
    out.push_str(rest);
    out
}

/// Renders Markdown that isn't a note in the vault (an AI answer, an old
/// version): links aren't looked up.
#[uniffi::export]
pub fn render_markdown(text: String) -> Vec<Block> {
    render_with(&text, &|_| None)
}

#[uniffi::export]
impl Marrow {
    /// The blocks of a note, with its links looked up in the vault.
    /// `text` is the note's text (as read); `path` is where it is.
    pub fn render_note(&self, path: String, text: String) -> Result<Vec<Block>> {
        let src = vp(&path)?;
        self.with(|e| {
            let cache = std::cell::RefCell::new(HashMap::<String, Option<String>>::new());
            let resolve = |link: &str| -> Option<String> {
                if let Some(hit) = cache.borrow().get(link) {
                    return hit.clone();
                }
                let (target, _) = marrow_core::parse::split_subpath(link);
                let found = if target.is_empty() { Some(src.to_string()) } else { e.resolve_link(link, &src).map(|p| p.to_string()) };
                cache.borrow_mut().insert(link.to_string(), found.clone());
                found
            };
            Ok(render_with(&text, &resolve))
        })
    }
}

// ------------------------------------------------------------ editor colours

#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum MarkKind {
    /// A heading line; `level` says which.
    Heading,
    Bold,
    Italic,
    Strike,
    Code,
    CodeBlock,
    Link,
    Tag,
    Quote,
    ListMarker,
    TaskOpen,
    /// A done task's text.
    TaskDone,
    Highlight,
    Frontmatter,
    /// Markdown's own symbols (`**`, `[[`, `#`), shown faint.
    Syntax,
}

#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct Mark {
    pub kind: MarkKind,
    /// UTF-16 offsets into the text.
    pub start: u32,
    pub end: u32,
    pub level: u8,
}

/// Colours for the editor: where headings, bold, links, tags and so on are.
#[uniffi::export]
pub fn highlight(text: String) -> Vec<Mark> {
    let mut out = Vec::new();
    let mut pos: u32 = 0; // UTF-16 offset of the line start
    let mut fence: Option<String> = None;
    let mut front = text.starts_with("---\n") || text.starts_with("---\r\n");
    for (n, raw) in text.split('\n').enumerate() {
        let line = raw.strip_suffix('\r').unwrap_or(raw);
        let len16 = raw.encode_utf16().count() as u32;
        let end = pos + line.encode_utf16().count() as u32;
        let mark = |kind, s: u32, e: u32, level: u8, out: &mut Vec<Mark>| {
            if e > s {
                out.push(Mark { kind, start: s, end: e, level });
            }
        };
        if front {
            mark(MarkKind::Frontmatter, pos, end, 0, &mut out);
            if n > 0 && line.trim_end() == "---" {
                front = false;
            }
        } else if let Some(f) = &fence {
            mark(MarkKind::CodeBlock, pos, end, 0, &mut out);
            if line.trim_start().starts_with(f.as_str()) {
                fence = None;
            }
        } else if line.trim_start().starts_with("```") || line.trim_start().starts_with("~~~") {
            fence = Some(line.trim_start()[..3].to_string());
            mark(MarkKind::CodeBlock, pos, end, 0, &mut out);
        } else {
            line_marks(line, pos, &mut out);
        }
        pos += len16 + 1;
    }
    out
}

fn line_marks(line: &str, base: u32, out: &mut Vec<Mark>) {
    let chars: Vec<char> = line.chars().collect();
    // UTF-16 offset of each char index (and one past the end).
    let mut at = Vec::with_capacity(chars.len() + 1);
    let mut o = base;
    for c in &chars {
        at.push(o);
        o += c.len_utf16() as u32;
    }
    at.push(o);
    let m = |kind, s: usize, e: usize, level: u8, out: &mut Vec<Mark>| {
        if e > s {
            out.push(Mark { kind, start: at[s], end: at[e], level });
        }
    };
    let n = chars.len();
    let mut i = 0;
    // Quote markers.
    while i < n && (chars[i] == '>' || (chars[i] == ' ' && i + 1 < n && chars[i + 1] == '>')) {
        i += 1;
    }
    if i > 0 && line.trim_start().starts_with('>') {
        m(MarkKind::Quote, 0, n, 0, out);
        m(MarkKind::Syntax, 0, i, 0, out);
        while i < n && chars[i] == ' ' {
            i += 1;
        }
    }
    // Heading.
    let hashes = chars[i..].iter().take_while(|c| **c == '#').count();
    if (1..=6).contains(&hashes) && (i + hashes == n || chars[i + hashes] == ' ') {
        m(MarkKind::Heading, i, n, hashes as u8, out);
        m(MarkKind::Syntax, i, i + hashes, hashes as u8, out);
        i += hashes;
    } else {
        // List marker and task box.
        let mut j = i;
        while j < n && chars[j] == ' ' || j < n && chars[j] == '\t' {
            j += 1;
        }
        let bullet = j < n && matches!(chars[j], '-' | '*' | '+') && j + 1 < n && chars[j + 1] == ' ';
        let digits = chars[j..].iter().take_while(|c| c.is_ascii_digit()).count();
        let numbered = digits > 0 && j + digits + 1 < n && matches!(chars[j + digits], '.' | ')') && chars[j + digits + 1] == ' ';
        if bullet || numbered {
            let mlen = if bullet { 1 } else { digits + 1 };
            m(MarkKind::ListMarker, j, j + mlen, 0, out);
            let k = j + mlen + 1;
            if k + 2 < n + 1 && k + 2 <= n && chars[k] == '[' && k + 2 < n && chars[k + 2] == ']' {
                let done = chars[k + 1] != ' ';
                m(if done { MarkKind::TaskDone } else { MarkKind::TaskOpen }, k, k + 3, 0, out);
                if done {
                    m(MarkKind::TaskDone, k + 3, n, 1, out);
                }
                i = k + 3;
            } else {
                i = k.min(n);
            }
        } else if chars[j..].iter().collect::<String>().trim() == "---" {
            m(MarkKind::Syntax, j, n, 0, out);
            return;
        }
    }
    inline_marks(&chars, i, &m, out);
}

/// Adds a mark between two char indexes.
type MarkFn<'a> = &'a dyn Fn(MarkKind, usize, usize, u8, &mut Vec<Mark>);

fn inline_marks(chars: &[char], from: usize, m: MarkFn, out: &mut Vec<Mark>) {
    let n = chars.len();
    let find = |from: usize, pat: &[char]| -> Option<usize> { (from..n.saturating_sub(pat.len() - 1)).find(|&k| chars[k..].starts_with(pat)) };
    let mut i = from;
    while i < n {
        let c = chars[i];
        // `code`
        if c == '`' {
            if let Some(e) = find(i + 1, &['`']) {
                m(MarkKind::Code, i, e + 1, 0, out);
                m(MarkKind::Syntax, i, i + 1, 0, out);
                m(MarkKind::Syntax, e, e + 1, 0, out);
                i = e + 1;
                continue;
            }
        }
        // [[wikilink]] and ![[embed]]
        if c == '[' && i + 1 < n && chars[i + 1] == '[' {
            if let Some(e) = find(i + 2, &[']', ']']) {
                let s = if i > 0 && chars[i - 1] == '!' { i - 1 } else { i };
                m(MarkKind::Link, i + 2, e, 0, out);
                m(MarkKind::Syntax, s, i + 2, 0, out);
                m(MarkKind::Syntax, e, e + 2, 0, out);
                // [[target|alias]]: the target part is faint.
                if let Some(bar) = (i + 2..e).find(|&k| chars[k] == '|') {
                    m(MarkKind::Syntax, i + 2, bar + 1, 0, out);
                }
                i = e + 2;
                continue;
            }
        }
        // [text](url)
        if c == '[' {
            if let Some(close) = find(i + 1, &[']', '(']) {
                if let Some(e) = find(close + 2, &[')']) {
                    let s = if i > 0 && chars[i - 1] == '!' { i - 1 } else { i };
                    m(MarkKind::Link, i + 1, close, 0, out);
                    m(MarkKind::Syntax, s, i + 1, 0, out);
                    m(MarkKind::Syntax, close, e + 1, 0, out);
                    i = e + 1;
                    continue;
                }
            }
        }
        // **bold**, __bold__, ~~strike~~, ==highlight==
        for (pat, kind) in [(['*', '*'], MarkKind::Bold), (['_', '_'], MarkKind::Bold), (['~', '~'], MarkKind::Strike), (['=', '='], MarkKind::Highlight)] {
            if c == pat[0] && i + 1 < n && chars[i + 1] == pat[1] && i + 2 < n && chars[i + 2] != ' ' {
                if let Some(e) = find(i + 2, &pat) {
                    m(kind, i, e + 2, 0, out);
                    m(MarkKind::Syntax, i, i + 2, 0, out);
                    m(MarkKind::Syntax, e, e + 2, 0, out);
                    inline_marks(&chars[..e], i + 2, m, out);
                    i = e + 2;
                    break;
                }
            }
        }
        if i < n && chars[i] != c {
            continue;
        }
        // *italic*, _italic_
        if (c == '*' || c == '_') && i + 1 < n && chars[i + 1] != ' ' && chars[i + 1] != c && (c == '*' || i == 0 || !chars[i - 1].is_alphanumeric()) {
            if let Some(e) = find(i + 1, &[c]) {
                if e > i + 1 && chars[e - 1] != ' ' {
                    m(MarkKind::Italic, i, e + 1, 0, out);
                    m(MarkKind::Syntax, i, i + 1, 0, out);
                    m(MarkKind::Syntax, e, e + 1, 0, out);
                    i = e + 1;
                    continue;
                }
            }
        }
        // #tag
        if c == '#' && (i == 0 || chars[i - 1].is_whitespace() || matches!(chars[i - 1], '(' | '[' | ',' | ';')) {
            let rest: String = chars[i + 1..].iter().collect();
            let len = marrow_core::parse::tag_len(&rest);
            if len > 0 {
                let count = rest[..len].chars().count();
                m(MarkKind::Tag, i, i + 1 + count, 0, out);
                i += 1 + count;
                continue;
            }
        }
        // Bare web addresses.
        if c == 'h' && (chars[i..].starts_with(&['h', 't', 't', 'p', ':', '/', '/']) || chars[i..].starts_with(&['h', 't', 't', 'p', 's', ':', '/', '/'])) {
            let e = (i..n).find(|&k| chars[k].is_whitespace()).unwrap_or(n);
            m(MarkKind::Link, i, e, 0, out);
            i = e;
            continue;
        }
        i += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plain(b: &Block) -> String {
        b.spans.iter().map(|s| s.text.as_str()).collect()
    }

    #[test]
    fn blocks_of_a_note() {
        let t = "---\ntags: [a, b]\nstatus: draft\n---\n# Title\n\nSome **bold** and [[Other|that]] #tag ==hi==\n\n- one\n  - two\n- [x] done\n1. first\n2. second\n\n> [!tip] Remember\n> Be kind.\n\n```rust\nfn main() {}\n```\n\n| a | b |\n|---|---|\n| 1 | 2 |\n\n![[pic.png]]\n\n---\n";
        let resolve = |l: &str| (l == "Other" || l == "pic.png").then(|| format!("dir/{l}"));
        let b = render_with(t, &resolve);
        let kinds: Vec<BlockKind> = b.iter().map(|x| x.kind).collect();
        assert_eq!(
            kinds,
            [
                BlockKind::Properties,
                BlockKind::Heading,
                BlockKind::Paragraph,
                BlockKind::Bullet,
                BlockKind::Bullet,
                BlockKind::Task,
                BlockKind::Numbered,
                BlockKind::Numbered,
                BlockKind::CalloutTitle,
                BlockKind::Paragraph,
                BlockKind::Code,
                BlockKind::Table,
                BlockKind::Image,
                BlockKind::Rule
            ]
        );
        assert_eq!(b[0].props[0], Property { key: "status".into(), value: "draft".into() });
        assert_eq!(b[1].level, 1);
        assert_eq!(b[1].line, 4);
        let p = &b[2];
        assert_eq!(plain(p), "Some bold and that #tag hi");
        assert!(p.spans.iter().any(|s| s.bold && s.text == "bold"));
        assert_eq!(p.spans.iter().find(|s| s.text == "that").unwrap().link, Some(SpanLink { kind: LinkKind::Note, target: "dir/Other".into() }));
        assert_eq!(p.spans.iter().find(|s| s.text == "#tag").unwrap().link.as_ref().unwrap().kind, LinkKind::Tag);
        assert!(p.spans.iter().find(|s| s.text == "hi").unwrap().highlight);
        assert_eq!((b[3].level, b[4].level), (0, 1));
        assert!(b[5].checked);
        assert_eq!(b[5].line, 10);
        assert_eq!((b[6].number, b[7].number), (1, 2));
        assert_eq!(b[8].callout, "tip");
        assert_eq!(plain(&b[8]), "Remember");
        assert_eq!((plain(&b[9]).as_str(), b[9].quote, b[9].callout.as_str()), ("Be kind.", 1, "tip"));
        assert_eq!((b[10].language.as_str(), b[10].text.as_str()), ("rust", "fn main() {}"));
        assert_eq!(b[11].rows.len(), 2);
        assert!(b[11].rows[0].header);
        assert_eq!((b[12].target.as_str(), b[12].local), ("dir/pic.png", true));
    }

    #[test]
    fn missing_links_and_inline_embeds() {
        let b = render_with("See [[Nope]] then ![[x.png]] after", &|_| None);
        assert_eq!(b[0].spans[1].link, Some(SpanLink { kind: LinkKind::Missing, target: "Nope".into() }));
        assert_eq!(b[1].kind, BlockKind::Image);
        assert_eq!(plain(&b[2]), "after");
        let b = render_with("- [ ] task with #a/b\n", &|_| None);
        assert_eq!(b[0].kind, BlockKind::Task);
        assert_eq!(b[0].spans[1].link.as_ref().unwrap().target, "#a/b");
        assert_eq!(plain(&render_with("a\nb", &|_| None)[0]), "a\nb");
        assert_eq!(render_with("x %% hidden %% y", &|_| None).len(), 1);
        assert_eq!(plain(&render_with("x %% hidden %% y", &|_| None)[0]), "x  y");
    }

    #[test]
    fn editor_marks() {
        let t = "# Hi\n**b** and [[L]] #t\n- [x] done\n";
        let marks = highlight(t.into());
        let has = |k: MarkKind, s: u32, e: u32| marks.iter().any(|m| m.kind == k && m.start == s && m.end == e);
        assert!(has(MarkKind::Heading, 0, 4));
        assert!(has(MarkKind::Bold, 5, 10));
        assert!(has(MarkKind::Link, 17, 18));
        assert!(has(MarkKind::Tag, 21, 23));
        assert!(has(MarkKind::TaskDone, 26, 29));
        // UTF-16: an emoji counts two.
        let marks = highlight("😀 #t".into());
        assert!(marks.iter().any(|m| m.kind == MarkKind::Tag && m.start == 3 && m.end == 5));
    }
}
