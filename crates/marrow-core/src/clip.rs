//! Web pages as notes ("clippings"). The browser extension sends a page's
//! HTML (or the part that was selected); phones fetch the page. Either way
//! the article is picked out of the page (menus, ads and footers left
//! behind) and turned into Markdown, with where it came from on top:
//!
//! ```text
//! ---
//! source: https://example.com/post
//! clipped: 2026-10-30
//! ---
//!
//! # The post's title
//!
//! The article…
//! ```

/// Where clippings go, unless the person picks another folder.
pub const FOLDER: &str = "Clippings";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Clip {
    pub title: String,
    /// The whole note: properties, title and the article.
    pub text: String,
}

/// Makes a note from a page.
///
/// - `html`: the page.
/// - `url`: where it is (to make its links and pictures work, and to note
///   the source).
/// - `selection`: just this part of the page (HTML), if some was selected.
/// - `today`: `YYYY-MM-DD`.
pub fn clip_html(html: &str, url: &str, selection: Option<&str>, today: &str) -> Clip {
    let base = url.starts_with("http://") || url.starts_with("https://");
    let mut r = dom_smoothie::Readability::new(html, base.then_some(url), None).ok();
    let page_title = r.as_ref().map(|r| r.get_article_title().to_string()).unwrap_or_default();

    let (title, body) = match selection.map(str::trim).filter(|s| !s.is_empty()) {
        Some(sel) => (page_title, to_markdown(&absolute(sel, url))),
        None => match r.as_mut().and_then(|r| r.parse().ok()) {
            Some(a) if !a.text_content.trim().is_empty() => {
                let t = if a.title.trim().is_empty() { page_title } else { a.title.clone() };
                (t, to_markdown(&a.content))
            }
            // Not an article (a search page, an app): keep all of the page's words.
            _ => (page_title, to_markdown(&absolute(html, url))),
        },
    };
    let mut title = one_line(&title);
    // "Post | Site": when the page also shows "Post" on its own (its
    // heading), that's the title.
    for sep in [" | ", " – ", " — ", " - ", " · ", " :: "] {
        if let Some((head, _)) = title.rsplit_once(sep) {
            let head = head.trim();
            if !head.is_empty() && (html.contains(&format!(">{head}<")) || body.contains(&format!("# {head}\n"))) {
                title = head.to_string();
                break;
            }
        }
    }
    let title = if title.is_empty() { host(url).unwrap_or("Clipping").to_string() } else { title };
    let body = drop_repeated_title(body.trim(), &title);

    let mut text = String::from("---\n");
    if !url.is_empty() {
        text.push_str(&format!("source: {}\n", yaml_value(url)));
    }
    text.push_str(&format!("clipped: {today}\n---\n\n# {title}\n\n"));
    if !body.is_empty() {
        text.push_str(body);
        text.push('\n');
    }
    Clip { title, text }
}

/// A file name for a clipping's title: no characters files or links can't
/// have, not too long, never empty.
pub fn file_name(title: &str) -> String {
    let cleaned: String = title
        .chars()
        .map(|c| if matches!(c, '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' | '#' | '^' | '[' | ']') || c.is_control() { ' ' } else { c })
        .collect();
    let mut name = cleaned.split_whitespace().collect::<Vec<_>>().join(" ");
    if name.chars().count() > 80 {
        name = name.chars().take(80).collect::<String>();
        // At a word's end, if one is near.
        if let Some(i) = name.rfind(' ').filter(|&i| i > 50) {
            name.truncate(i);
        }
    }
    let name = name.trim_matches(|c: char| c == '.' || c.is_whitespace()).to_string();
    if name.is_empty() { "Clipping".into() } else { name }
}

fn to_markdown(html: &str) -> String {
    let md = htmd::HtmlToMarkdown::builder()
        .skip_tags(vec!["script", "style", "noscript", "iframe", "nav", "footer", "form", "button", "svg"])
        .build()
        .convert(html)
        .unwrap_or_default();
    tidy(&md)
}

/// No runs of empty lines, no spaces at line ends.
fn tidy(md: &str) -> String {
    let mut out = String::with_capacity(md.len());
    let mut blank = 0;
    for line in md.lines() {
        let line = line.trim_end();
        if line.is_empty() {
            blank += 1;
            if blank > 1 {
                continue;
            }
        } else {
            blank = 0;
        }
        out.push_str(line);
        out.push('\n');
    }
    out.trim().to_string()
}

/// The article often starts with its own title: don't show it twice.
fn drop_repeated_title<'a>(body: &'a str, title: &str) -> &'a str {
    let first = body.lines().next().unwrap_or_default();
    let words = first.trim_start_matches('#').trim();
    if first.starts_with('#') && words.eq_ignore_ascii_case(title) {
        body[first.len()..].trim_start()
    } else {
        body
    }
}

/// Links and pictures in a selection, made to point at the page's site.
fn absolute(html: &str, url: &str) -> String {
    let Some(origin) = origin(url) else { return html.to_string() };
    let dir = url.split(['?', '#']).next().unwrap_or(url);
    let dir = &dir[..dir.rfind('/').map(|i| i + 1).unwrap_or(dir.len())];
    let mut out = String::with_capacity(html.len());
    let mut rest = html;
    while let Some(i) = find_attr(rest) {
        let (before, after) = rest.split_at(i);
        out.push_str(before);
        let eq = after.find('=').unwrap() + 1;
        let quote = after[eq..].chars().next().unwrap_or('"');
        let start = eq + 1;
        let end = after[start..].find(quote).map(|e| start + e).unwrap_or(after.len());
        let v = &after[start..end.min(after.len())];
        let fixed = if v.starts_with("//") {
            format!("{}:{v}", url.split(':').next().unwrap_or("https"))
        } else if v.starts_with('/') {
            format!("{origin}{v}")
        } else if v.contains(':') || v.starts_with('#') || v.is_empty() {
            v.to_string()
        } else {
            format!("{dir}{v}")
        };
        out.push_str(&after[..start]);
        out.push_str(&fixed);
        rest = &after[end.min(after.len())..];
    }
    out.push_str(rest);
    out
}

/// Where the next ` href="` or ` src="` is.
fn find_attr(s: &str) -> Option<usize> {
    let lower = s.to_ascii_lowercase();
    ["href=\"", "src=\"", "href='", "src='"]
        .iter()
        .filter_map(|a| lower.find(&format!(" {a}")).map(|i| i + 1))
        .min()
}

fn origin(url: &str) -> Option<String> {
    let (scheme, rest) = url.split_once("://")?;
    let host = rest.split(['/', '?', '#']).next()?;
    (!host.is_empty()).then(|| format!("{scheme}://{host}"))
}

fn host(url: &str) -> Option<&str> {
    let rest = url.split_once("://")?.1;
    rest.split(['/', '?', '#']).next().filter(|h| !h.is_empty())
}

fn one_line(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// A value for the properties block: quoted when YAML would misread it.
fn yaml_value(v: &str) -> String {
    if v.contains(": ") || v.contains(" #") || v.starts_with(['"', '\'', '[', '{', '&', '*', '!', '|', '>', '%', '@', '`']) {
        format!("\"{}\"", v.replace('\\', "\\\\").replace('"', "\\\""))
    } else {
        v.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const PAGE: &str = r#"<!doctype html><html><head><title>How to grow potatoes | Garden Weekly</title></head>
<body><nav><a href="/">Home</a> <a href="/about">About</a></nav>
<article><h1>How to grow potatoes</h1>
<p>Potatoes are one of the easiest crops to grow. Plant <strong>seed potatoes</strong> in early spring, about
30 cm apart, in rows 60 cm apart. Cover them with soil as the shoots come up.</p>
<p>Water them well when the flowers open, because that is when the tubers grow. Read our <a href="/guides/soil">soil guide</a>
for more about getting the ground ready, and see the picture below.</p>
<img src="img/rows.jpg" alt="Rows of potatoes">
<p>Lift them when the leaves turn yellow and die back, usually in late summer. Leave them to dry for a few hours
before you store them somewhere dark and cool.</p></article>
<footer>© Garden Weekly. Subscribe to our newsletter!</footer></body></html>"#;

    #[test]
    fn article_as_a_note() {
        let c = clip_html(PAGE, "https://garden.example/guides/potatoes", None, "2026-10-30");
        assert_eq!(c.title, "How to grow potatoes");
        assert!(c.text.starts_with("---\nsource: https://garden.example/guides/potatoes\nclipped: 2026-10-30\n---\n\n# How to grow potatoes\n\n"), "{}", c.text);
        assert!(c.text.contains("**seed potatoes**"));
        assert!(c.text.contains("[soil guide](https://garden.example/guides/soil)"), "{}", c.text);
        assert!(c.text.contains("https://garden.example/guides/img/rows.jpg"), "{}", c.text);
        assert!(!c.text.contains("Subscribe"), "{}", c.text);
        assert!(!c.text.contains("About"), "{}", c.text);
        // The title isn't repeated under itself.
        assert_eq!(c.text.matches("How to grow potatoes").count(), 1, "{}", c.text);
    }

    #[test]
    fn just_the_selection() {
        let sel = r#"<p>Water them well <a href="/guides/soil">soil</a></p>"#;
        let c = clip_html(PAGE, "https://garden.example/guides/potatoes", Some(sel), "2026-10-30");
        assert!(c.text.ends_with("# How to grow potatoes | Garden Weekly\n\nWater them well [soil](https://garden.example/guides/soil)\n") || c.text.contains("Water them well [soil](https://garden.example/guides/soil)"), "{}", c.text);
        assert!(!c.text.contains("Lift them"));
    }

    #[test]
    fn not_an_article() {
        let c = clip_html("<html><body><p>Hi</p></body></html>", "https://x.example/a?b:c", None, "2026-10-30");
        assert_eq!(c.title, "x.example");
        assert!(c.text.contains("Hi"));
        assert!(c.text.contains("source: https://x.example/a?b:c\n"));
    }

    #[test]
    fn names() {
        assert_eq!(file_name("How to grow potatoes | Garden Weekly"), "How to grow potatoes Garden Weekly");
        assert_eq!(file_name("  ...  "), "Clipping");
        assert_eq!(file_name("a/b: c?"), "a b c");
        let long = "word ".repeat(40);
        assert!(file_name(&long).len() <= 80);
        assert_eq!(yaml_value("https://x.example/#a"), "https://x.example/#a");
        assert_eq!(yaml_value("a: b"), "\"a: b\"");
    }
}
