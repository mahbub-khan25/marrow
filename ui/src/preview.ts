// Reading view: Markdown to sanitised HTML, with the Obsidian-flavoured bits
// (wikilinks, embeds, #tags, task checkboxes, callouts, ==highlights==).
//
// Every block carries `data-line` (0-based, counting frontmatter) so the
// outline, search hits and double-click-to-edit can find their place.

import MarkdownIt from "markdown-it";
import type StateCore from "markdown-it/lib/rules_core/state_core.mjs";
import type StateInline from "markdown-it/lib/rules_inline/state_inline.mjs";
import type Token from "markdown-it/lib/token.mjs";
import DOMPurify from "dompurify";
import footnote from "markdown-it-footnote";

import { calloutKind } from "./callouts";
import { mathPlugin } from "./math";
import { stripFrontmatter } from "./util";

export interface RenderEnv {
  /** Lines the frontmatter took, added to every data-line. */
  lineOffset: number;
  /** Lower-cased link targets that point nowhere. */
  unresolved: Set<string>;
  /** Embeds are rendered one level deep only. */
  depth: number;
}

const IMAGE_EXT = /\.(png|jpe?g|gif|webp|svg|bmp|avif|ico)$/i;

export function linkTarget(inner: string): string {
  return inner.split("|")[0].replace(/\\$/, "").split("#")[0].trim();
}

const esc = (s: string) =>
  s.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;").replace(/"/g, "&quot;");

// ------------------------------------------------------------ inline rules

function wikilinkRule(state: StateInline, silent: boolean): boolean {
  const src = state.src;
  let pos = state.pos;
  const embed = src.charCodeAt(pos) === 0x21; /* ! */
  if (embed) pos++;
  if (src.charCodeAt(pos) !== 0x5b || src.charCodeAt(pos + 1) !== 0x5b) return false;
  const end = src.indexOf("]]", pos + 2);
  if (end < 0) return false;
  const inner = src.slice(pos + 2, end);
  if (!inner || inner.includes("\n") || inner.includes("[")) return false;
  if (!silent) {
    const t = state.push(embed ? "wiki_embed" : "wikilink", "", 0);
    t.content = inner;
  }
  state.pos = end + 2;
  return true;
}

const TAG_BODY = /^#((?![\d]+(?![\p{L}\p{N}_\-/]))[\p{L}\p{N}_\-/]+)/u;

function tagRule(state: StateInline, silent: boolean): boolean {
  const pos = state.pos;
  if (state.src.charCodeAt(pos) !== 0x23 /* # */) return false;
  const prev = pos > 0 ? state.src[pos - 1] : " ";
  if (!/[\s(\[,;]/.test(prev)) return false;
  const m = TAG_BODY.exec(state.src.slice(pos));
  if (!m) return false;
  if (!silent) {
    const t = state.push("hashtag", "", 0);
    t.content = m[1];
  }
  state.pos += m[0].length;
  return true;
}

function highlightRule(state: StateInline, silent: boolean): boolean {
  const pos = state.pos;
  const src = state.src;
  if (src.charCodeAt(pos) !== 0x3d || src.charCodeAt(pos + 1) !== 0x3d) return false;
  const end = src.indexOf("==", pos + 2);
  if (end < 0 || end === pos + 2) return false;
  if (!silent) {
    state.push("mark_open", "mark", 1);
    const oldMax = state.posMax;
    state.pos = pos + 2;
    state.posMax = end;
    state.md.inline.tokenize(state);
    state.posMax = oldMax;
    state.push("mark_close", "mark", -1);
  }
  state.pos = end + 2;
  return true;
}

// -------------------------------------------------------------- core rules

/** `data-line` on every block that knows where it came from. */
function lineAttrs(state: StateCore) {
  const off = (state.env as RenderEnv).lineOffset ?? 0;
  for (const t of state.tokens) {
    if (t.map && (t.nesting === 1 || t.type === "fence" || t.type === "code_block" || t.type === "hr" || t.type === "html_block" || t.type === "math_block")) {
      t.attrSet("data-line", String(t.map[0] + off));
    }
  }
}

/** `- [ ] item` / `- [x] item` become checkable tasks. */
function tasks(state: StateCore) {
  const toks = state.tokens;
  const off = (state.env as RenderEnv).lineOffset ?? 0;
  for (let i = 0; i < toks.length; i++) {
    const t = toks[i];
    if (t.type !== "list_item_open") continue;
    const inline = toks[i + 2];
    if (!inline || toks[i + 1].type !== "paragraph_open" || inline.type !== "inline") continue;
    const m = /^\[([ xX])\][ \t]/.exec(inline.content) ?? /^\[([ xX])\]$/.exec(inline.content);
    if (!m) continue;
    const done = m[1] !== " ";
    const line = (t.map?.[0] ?? 0) + off;
    t.meta = { task: true, done, line };
    // Find the matching close.
    for (let j = i + 1, depth = 0; j < toks.length; j++) {
      if (toks[j].type === "list_item_open") depth++;
      else if (toks[j].type === "list_item_close") {
        if (depth === 0) {
          toks[j].meta = { task: true };
          break;
        }
        depth--;
      }
    }
    // Drop "[ ] " from the text.
    const first = inline.children?.[0];
    if (first && first.type === "text") first.content = first.content.replace(/^\[[ xX]\][ \t]?/, "");
    inline.content = inline.content.replace(/^\[[ xX]\][ \t]?/, "");
  }
}

/** `> [!note] Title` blockquotes become callouts. */
function callouts(state: StateCore) {
  const toks = state.tokens;
  for (let i = 0; i < toks.length; i++) {
    if (toks[i].type !== "blockquote_open") continue;
    const inline = toks[i + 2];
    if (!inline || toks[i + 1].type !== "paragraph_open" || inline.type !== "inline") continue;
    const m = /^\[!([\w-]+)\]([+-]?)[ \t]*([^\n]*)/.exec(inline.content);
    if (!m) continue;
    const type = m[1].toLowerCase();
    toks[i].meta = { callout: type, fold: m[2], title: m[3].trim() || calloutKind(type).label };
    // Remove the first line from the paragraph.
    const kids = inline.children ?? [];
    const br = kids.findIndex((k) => k.type === "softbreak" || k.type === "hardbreak");
    inline.children = br >= 0 ? kids.slice(br + 1) : [];
    inline.content = inline.content.includes("\n") ? inline.content.slice(inline.content.indexOf("\n") + 1) : "";
    // Find the close.
    for (let j = i + 1, depth = 0; j < toks.length; j++) {
      if (toks[j].type === "blockquote_open") depth++;
      else if (toks[j].type === "blockquote_close") {
        if (depth === 0) {
          toks[j].meta = { callout: type, fold: m[2] };
          break;
        }
        depth--;
      }
    }
  }
}

export interface ButtonSpec {
  name: string;
  /** "append" (below the button), "new-note", or "daily" (append to today's note). */
  action: string;
  template: string;
  folder: string;
}

/** A ```button block: "key: value" lines. */
export function buttonSpec(body: string): ButtonSpec {
  const spec: ButtonSpec = { name: "", action: "append", template: "", folder: "" };
  for (const line of body.split("\n")) {
    const m = /^\s*(\w+)\s*:\s*(.*?)\s*$/.exec(line);
    if (!m) continue;
    const k = m[1].toLowerCase();
    const v = m[2].replace(/^\[\[(.*)\]\]$/, "$1").replace(/^["']|["']$/g, "");
    if (k === "name" || k === "action" || k === "template" || k === "folder") spec[k] = v;
  }
  spec.action = spec.action.toLowerCase();
  return spec;
}

// ---------------------------------------------------------------- renderer

function makeMd(): MarkdownIt {
  const md = new MarkdownIt({ html: true, linkify: true, breaks: true, typographer: false });
  md.inline.ruler.before("link", "wikilink", wikilinkRule);
  md.inline.ruler.before("emphasis", "hashtag", tagRule);
  md.inline.ruler.before("emphasis", "highlight", highlightRule);
  md.core.ruler.after("inline", "tasks", tasks);
  md.core.ruler.after("tasks", "callouts", callouts);
  md.use(mathPlugin);
  md.use(footnote);
  md.core.ruler.push("line_attrs", lineAttrs);

  const r = md.renderer.rules;

  r.wikilink = (toks: Token[], i: number, _o, env: RenderEnv) => {
    const inner = toks[i].content;
    const target = linkTarget(inner);
    const alias = inner.includes("|") ? inner.slice(inner.indexOf("|") + 1) : null;
    const sub = inner.split("|")[0].includes("#") ? inner.split("|")[0].split("#").slice(1).join("#") : "";
    const label = alias ?? (target ? (sub ? `${target} › ${sub}` : target) : sub);
    const missing = target !== "" && env.unresolved.has(target.replace(/\.md$/i, "").toLowerCase());
    return `<a class="internal${missing ? " unresolved" : ""}" data-link="${esc(inner.split("|")[0])}">${esc(label)}</a>`;
  };

  r.wiki_embed = (toks: Token[], i: number, _o, env: RenderEnv) => {
    const inner = toks[i].content;
    const target = linkTarget(inner);
    const size = inner.includes("|") ? inner.slice(inner.indexOf("|") + 1).trim() : "";
    if (IMAGE_EXT.test(target)) {
      const w = /^\d+(x\d+)?$/.test(size) ? ` width="${size.split("x")[0]}"` : "";
      return `<img data-embed="${esc(target)}" alt="${esc(size && !w ? size : target)}"${w}>`;
    }
    const isNote = !/\.[a-z0-9]{1,5}$/i.test(target) || /\.md$/i.test(target);
    if (isNote && env.depth < 1) {
      return `<div class="embed" data-embed-note="${esc(inner.split("|")[0])}"></div>`;
    }
    return `<a class="internal" data-link="${esc(inner.split("|")[0])}">${esc(target)}</a>`;
  };

  r.hashtag = (toks: Token[], i: number) => {
    const tag = toks[i].content;
    return `<span class="tag-link" data-tag="${esc(tag)}">#${esc(tag)}</span>`;
  };

  const defaultListOpen = r.list_item_open ?? ((t, i, o, _e, self) => self.renderToken(t, i, o));
  r.list_item_open = (toks, i, o, env, self) => {
    const t = toks[i];
    if (t.meta?.task) {
      const { done, line } = t.meta as { done: boolean; line: number };
      return `<li class="task${done ? " done" : ""}" data-line="${line}"><span class="check" data-task="${line}" role="checkbox" aria-checked="${done}">${done ? "✓" : ""}</span><div class="body">`;
    }
    return defaultListOpen(toks, i, o, env, self);
  };
  const defaultListClose = r.list_item_close ?? ((t, i, o, _e, self) => self.renderToken(t, i, o));
  r.list_item_close = (toks, i, o, env, self) => {
    if (toks[i].meta?.task) return "</div></li>\n";
    return defaultListClose(toks, i, o, env, self);
  };

  r.blockquote_open = (toks, i, o, _env, self) => {
    const m = toks[i].meta as { callout?: string; fold?: string; title?: string } | null;
    if (m?.callout && m.title !== undefined) {
      const line = toks[i].attrGet("data-line");
      const k = calloutKind(m.callout);
      const attrs = `class="callout" data-callout="${esc(k.type)}" style="--co:${k.color}"${line ? ` data-line="${line}"` : ""}`;
      const icon = `<i class="ph-duotone ph-${k.icon} callout-icon" title="Change callout type"></i>`;
      // "[!note]-" starts folded, "[!note]+" starts open; both can be toggled.
      if (m.fold) return `<details ${attrs}${m.fold === "+" ? " open" : ""}><summary class="callout-title">${icon}${esc(m.title)}</summary>\n`;
      return `<div ${attrs}><div class="callout-title">${icon}${esc(m.title)}</div>\n`;
    }
    return self.renderToken(toks, i, o);
  };
  r.blockquote_close = (toks, i, o, _env, self) => {
    const m = toks[i].meta as { callout?: string; fold?: string } | null;
    if (m?.callout) return m.fold ? "</details>\n" : "</div>\n";
    return self.renderToken(toks, i, o);
  };

  r.link_open = (toks, i, o, _env, self) => {
    const t = toks[i];
    const href = t.attrGet("href") ?? "";
    if (/^[a-z][a-z0-9+.-]*:/i.test(href)) {
      t.attrSet("class", "external");
      t.attrSet("data-href", href);
    } else if (href.startsWith("#")) {
      t.attrSet("class", "internal");
      t.attrSet("data-heading", href.slice(1));
    } else {
      let target = href;
      try {
        target = decodeURI(href);
      } catch {
        /* keep as written */
      }
      t.attrSet("class", "internal");
      t.attrSet("data-link", target);
    }
    // Navigation is handled by the click handler, never by the webview.
    t.attrs = (t.attrs ?? []).filter(([k]) => k !== "href");
    return self.renderToken(toks, i, o);
  };

  r.image = (toks, i, o, env, self) => {
    const t = toks[i];
    const src = t.attrGet("src") ?? "";
    if (!/^[a-z][a-z0-9+.-]*:/i.test(src)) {
      let target = src;
      try {
        target = decodeURI(src);
      } catch {
        /* keep */
      }
      t.attrs = (t.attrs ?? []).filter(([k]) => k !== "src");
      t.attrSet("data-embed", target);
    }
    t.attrSet("alt", self.renderInlineAsText(t.children ?? [], o, env));
    return self.renderToken(toks, i, o);
  };

  const defaultFence = r.fence!;
  r.fence = (toks, i, o, env, self) => {
    const line = toks[i].attrGet("data-line");
    const info = toks[i].info.trim().toLowerCase();
    if (info === "button" && env.depth < 1) {
      const spec = buttonSpec(toks[i].content);
      return `<div class="md-button"${line ? ` data-line="${line}"` : ""}><span class="md-btn" role="button" tabindex="0" data-button="${esc(JSON.stringify(spec))}">${esc(spec.name || "Button")}</span></div>\n`;
    }
    if (info === "mermaid") {
      const block = `<div class="mermaid-block" data-mermaid="${encodeURIComponent(toks[i].content)}"><div class="db-loading">Drawing diagram…</div></div>`;
      return `<div class="mermaid-wrap"${line ? ` data-line="${line}"` : ""}>${block}</div>\n`;
    }
    if (info === "database" && line && env.depth < 1) {
      return `<div class="db" data-line="${line}" data-db="${esc(toks[i].content)}"><div class="db-loading">Loading…</div></div>\n`;
    }
    if (info === "columns" && line) {
      // Parts separated by a line of "+++"; each is ordinary Markdown.
      const start = Number(line) + 1;
      const parts: { text: string; at: number }[] = [];
      let cur: string[] = [];
      let at = start;
      toks[i].content.split("\n").forEach((l, n) => {
        if (/^\s*\+\+\+\s*$/.test(l)) {
          parts.push({ text: cur.join("\n"), at });
          cur = [];
          at = start + n + 1;
        } else cur.push(l);
      });
      parts.push({ text: cur.join("\n"), at });
      const cols = parts
        .map((p) => `<div class="md-col">${md.render(p.text, { ...env, lineOffset: p.at })}</div>`)
        .join("");
      return `<div class="md-columns" data-line="${line}" style="--cols:${parts.length}">${cols}</div>\n`;
    }
    const html = defaultFence(toks, i, o, env, self);
    return line ? html.replace("<pre", `<pre data-line="${line}"`) : html;
  };

  return md;
}

const md = makeMd();

/** Block ids (` ^id`) aren't shown when reading; line numbers stay the same. */
function hideBlockIds(text: string): string {
  if (!text.includes("^")) return text;
  let fence = "";
  let math = false;
  return text
    .split("\n")
    .map((l) => {
      // Math blocks keep their ^ (x ^2 isn't a block id).
      const t = l.trim();
      if (!fence && !math && t.startsWith("$$")) {
        if (!(t.length >= 4 && t.endsWith("$$"))) math = true;
        return l;
      }
      if (math) {
        if (t.endsWith("$$")) math = false;
        return l;
      }
      const f = /^\s*(```+|~~~+)/.exec(l);
      if (f) {
        if (!fence) fence = f[1][0];
        else if (f[1][0] === fence) fence = "";
        return l;
      }
      if (fence) return l;
      if (/^\s*\^[A-Za-z0-9-]+\s*$/.test(l)) return "";
      return l.replace(/\s\^[A-Za-z0-9-]+\s*$/, "");
    })
    .join("\n");
}

/** Renders a note. `text` is the whole file, frontmatter included. */
export function renderMarkdown(text: string, unresolved: Set<string>, depth = 0): string {
  const fm = stripFrontmatter(hideBlockIds(text));
  const env: RenderEnv = { lineOffset: fm.lines, unresolved, depth };
  const html = md.render(fm.text, env);
  return DOMPurify.sanitize(html, {
    ADD_ATTR: ["data-line", "data-link", "data-tag", "data-task", "data-embed", "data-embed-note", "data-href", "data-heading", "data-callout", "data-button", "data-db", "data-mermaid", "open"],
    FORBID_TAGS: ["style", "form", "input", "button", "textarea", "select"],
  });
}

