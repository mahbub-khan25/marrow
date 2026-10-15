// Visual formatting for the Source editor: a toolbar, a small bar over
// selected text, and Ctrl+B / Ctrl+I. Every button writes ordinary Markdown
// (**bold**, # Heading, - list…), so typing the marks by hand works too.

import { EditorSelection, StateField, type EditorState } from "@codemirror/state";
import { EditorView, keymap, showTooltip, type Tooltip } from "@codemirror/view";

import { aiOnEditor } from "./ai-panel";
import { h, icon } from "./util";

// ------------------------------------------------------------- inline marks

/** Wraps the selection in `mark` (or removes it if it's already there). */
export function toggleMark(view: EditorView, mark: string, close = mark) {
  const { state } = view;
  const tr = state.changeByRange((r) => {
    const before = state.sliceDoc(r.from - mark.length, r.from);
    const after = state.sliceDoc(r.to, r.to + close.length);
    if (before === mark && after === close) {
      return {
        changes: [
          { from: r.from - mark.length, to: r.from },
          { from: r.to, to: r.to + close.length },
        ],
        range: EditorSelection.range(r.from - mark.length, r.to - mark.length),
      };
    }
    const text = state.sliceDoc(r.from, r.to);
    if (text.length >= mark.length + close.length && text.startsWith(mark) && text.endsWith(close)) {
      return {
        changes: { from: r.from, to: r.to, insert: text.slice(mark.length, text.length - close.length) },
        range: EditorSelection.range(r.from, r.to - mark.length - close.length),
      };
    }
    if (r.empty) {
      // No selection: the word under the cursor, or an empty pair to type into.
      const word = state.wordAt(r.head);
      if (word) {
        return {
          changes: [
            { from: word.from, insert: mark },
            { from: word.to, insert: close },
          ],
          range: EditorSelection.cursor(r.head + mark.length),
        };
      }
      return { changes: { from: r.from, insert: mark + close }, range: EditorSelection.cursor(r.from + mark.length) };
    }
    return {
      changes: [
        { from: r.from, insert: mark },
        { from: r.to, insert: close },
      ],
      range: EditorSelection.range(r.from + mark.length, r.to + mark.length),
    };
  });
  view.dispatch(state.update(tr, { scrollIntoView: true, userEvent: "input.format" }));
  view.focus();
}

/** Turns the selection into a link: `[[note]]` or `[text](url)`. */
export function makeLink(view: EditorView, kind: "note" | "web") {
  const { state } = view;
  const r = state.selection.main;
  const text = state.sliceDoc(r.from, r.to);
  if (kind === "note") {
    const insert = `[[${text}]]`;
    view.dispatch({
      changes: { from: r.from, to: r.to, insert },
      // Cursor inside the brackets, so the note suggestions open.
      selection: text ? EditorSelection.cursor(r.from + insert.length) : EditorSelection.cursor(r.from + 2),
      userEvent: "input.format",
    });
  } else {
    const label = text || "link text";
    const insert = `[${label}](https://)`;
    const urlAt = r.from + label.length + 3;
    view.dispatch({
      changes: { from: r.from, to: r.to, insert },
      selection: text ? EditorSelection.range(urlAt, urlAt + 8) : EditorSelection.range(r.from + 1, r.from + 1 + label.length),
      userEvent: "input.format",
    });
  }
  view.focus();
}

// ------------------------------------------------------------- line kinds

export type LineKind = "p" | "h1" | "h2" | "h3" | "h4" | "h5" | "h6" | "ul" | "ol" | "task" | "quote";

const PREFIX = /^(\s*)(#{1,6}\s+|>\s?|[-*+]\s+\[[ xX]\]\s+|[-*+]\s+|\d+[.)]\s+)?/;

function kindOfLine(text: string): LineKind {
  const p = PREFIX.exec(text)?.[2] ?? "";
  if (/^#{1,6}\s/.test(p)) return `h${p.trim().length}` as LineKind;
  if (p.startsWith(">")) return "quote";
  if (/\[[ xX]\]/.test(p)) return "task";
  if (/^[-*+]/.test(p)) return "ul";
  if (/^\d/.test(p)) return "ol";
  return "p";
}

/** What the line with the cursor is, for the heading picker. */
export function currentKind(state: EditorState): LineKind {
  return kindOfLine(state.doc.lineAt(state.selection.main.head).text);
}

/** Makes every selected line a heading, list item, quote… (or plain text again). */
export function setLineKind(view: EditorView, kind: LineKind) {
  const { state } = view;
  const lines = new Set<number>();
  for (const r of state.selection.ranges) {
    const a = state.doc.lineAt(r.from).number;
    const b = state.doc.lineAt(r.to).number;
    for (let n = a; n <= b; n++) lines.add(n);
  }
  const nums = [...lines].sort((x, y) => x - y);
  // Pressing the same button again turns it off.
  const all = nums.every((n) => kindOfLine(state.doc.line(n).text) === kind);
  const target: LineKind = all && kind !== "p" ? "p" : kind;
  const changes = nums.map((n, i) => {
    const line = state.doc.line(n);
    const m = PREFIX.exec(line.text)!;
    const indent = m[1];
    const old = m[2] ?? "";
    const prefix =
      target === "p"
        ? ""
        : target.startsWith("h")
          ? `${"#".repeat(Number(target[1]))} `
          : target === "ul"
            ? "- "
            : target === "ol"
              ? `${i + 1}. `
              : target === "task"
                ? "- [ ] "
                : "> ";
    return { from: line.from + indent.length, to: line.from + indent.length + old.length, insert: prefix };
  });
  view.dispatch({ changes, userEvent: "input.format", scrollIntoView: true });
  view.focus();
}

/** Puts `text` on lines of its own at the cursor (tables, dividers, code). */
export function insertBlock(view: EditorView, text: string, cursorAt?: number) {
  const { state } = view;
  const r = state.selection.main;
  const line = state.doc.lineAt(r.from);
  const sel = state.sliceDoc(r.from, r.to);
  let body = text.replace("$SEL", sel);
  const before = line.text.trim() && r.from === line.to ? "\n\n" : line.text.trim() && r.from > line.from ? "\n\n" : "";
  body = before + body;
  const after = "\n";
  view.dispatch({
    changes: { from: r.from, to: r.to, insert: body + after },
    selection: EditorSelection.cursor(r.from + before.length + (cursorAt ?? body.length - before.length)),
    userEvent: "input.format",
    scrollIntoView: true,
  });
  view.focus();
}

// ------------------------------------------------------------------- UI

interface Button {
  icon: string;
  title: string;
  run: (v: EditorView) => void;
}

const MARKS: Button[] = [
  { icon: "text-b", title: "Bold (Ctrl+B)", run: (v) => toggleMark(v, "**") },
  { icon: "text-italic", title: "Italic (Ctrl+I)", run: (v) => toggleMark(v, "*") },
  { icon: "text-strikethrough", title: "Strikethrough (Ctrl+Shift+X)", run: (v) => toggleMark(v, "~~") },
  { icon: "highlighter", title: "Highlight (Ctrl+Shift+H)", run: (v) => toggleMark(v, "==") },
  { icon: "code", title: "Inline code", run: (v) => toggleMark(v, "`") },
];

const LINKS: Button[] = [
  { icon: "link-simple", title: "Link to a note", run: (v) => makeLink(v, "note") },
  { icon: "globe-simple", title: "Web link", run: (v) => makeLink(v, "web") },
];

const BLOCKS: Button[] = [
  { icon: "list-bullets", title: "Bulleted list", run: (v) => setLineKind(v, "ul") },
  { icon: "list-numbers", title: "Numbered list", run: (v) => setLineKind(v, "ol") },
  { icon: "check-square", title: "To-do list", run: (v) => setLineKind(v, "task") },
  { icon: "quotes", title: "Quote", run: (v) => setLineKind(v, "quote") },
];

const INSERTS: Button[] = [
  { icon: "code-block", title: "Code block", run: (v) => insertBlock(v, "```\n$SEL\n```", 4) },
  { icon: "table", title: "Table", run: (v) => insertBlock(v, "| Column | Column |\n| --- | --- |\n|  |  |", 2) },
  { icon: "minus", title: "Divider", run: (v) => insertBlock(v, "---") },
];

const KINDS: [LineKind, string][] = [
  ["p", "Normal text"],
  ["h1", "Heading 1"],
  ["h2", "Heading 2"],
  ["h3", "Heading 3"],
  ["h4", "Heading 4"],
];

function button(b: Button, view: () => EditorView): HTMLElement {
  const el = h("button.fmt-btn", { title: b.title, "aria-label": b.title, type: "button" }, icon(b.icon));
  // Keep the editor's selection: act on mousedown, don't take focus.
  el.addEventListener("mousedown", (e) => {
    e.preventDefault();
    b.run(view());
  });
  return el;
}

/** The toolbar above the Source editor. */
export class FormatBar {
  readonly el: HTMLElement;
  private kind: HTMLSelectElement;

  constructor(private view: () => EditorView) {
    this.kind = h(
      "select.fmt-kind",
      { title: "Text style", "aria-label": "Text style" },
      ...KINDS.map(([k, label]) => h("option", { value: k }, label)),
    ) as HTMLSelectElement;
    this.kind.addEventListener("change", () => setLineKind(this.view(), this.kind.value as LineKind));
    const sep = () => h("span.fmt-sep");
    this.el = h(
      "div.fmt-bar",
      { role: "toolbar", "aria-label": "Formatting" },
      this.kind,
      sep(),
      ...MARKS.map((b) => button(b, view)),
      sep(),
      ...LINKS.map((b) => button(b, view)),
      sep(),
      ...BLOCKS.map((b) => button(b, view)),
      sep(),
      ...INSERTS.map((b) => button(b, view)),
    );
  }

  /** Shows what the cursor's line is (heading, normal text…). */
  sync(state: EditorState) {
    const k = currentKind(state);
    this.kind.value = /^h[1-4]$/.test(k) ? k : "p";
  }
}

/** The small bar over selected text. */
const bubble = StateField.define<Tooltip | null>({
  create: () => null,
  update(tip, tr) {
    if (!tr.docChanged && !tr.selection) return tip;
    const r = tr.state.selection.main;
    if (r.empty || tr.state.selection.ranges.length > 1) return null;
    // Not across many lines; that's what the toolbar is for.
    if (tr.state.doc.lineAt(r.from).number !== tr.state.doc.lineAt(r.to).number && r.to - r.from > 400) return null;
    return {
      pos: r.from,
      end: r.to,
      above: true,
      arrow: false,
      create: (view) => {
        const ai = button({ icon: "sparkle", title: "Ask AI (Ctrl+J)", run: (v) => aiOnEditor(v) }, () => view);
        ai.classList.add("fmt-ai");
        ai.append(h("span", null, "AI"));
        const dom = h(
          "div.fmt-bubble",
          { role: "toolbar", "aria-label": "Format selection" },
          ai,
          h("span.fmt-sep"),
          ...[...MARKS, ...LINKS, BLOCKS[0]].map((b) => button(b, () => view)),
          h("span.fmt-sep"),
          ...(["h1", "h2", "h3"] as LineKind[]).map((k) =>
            button({ icon: `text-h-${["one", "two", "three"][Number(k[1]) - 1]}`, title: `Heading ${k[1]}`, run: (v) => setLineKind(v, k) }, () => view),
          ),
        );
        return { dom, offset: { x: 0, y: 6 } };
      },
    };
  },
  provide: (f) => showTooltip.from(f),
});

/** Bubble and shortcuts, for the editor's extensions. */
export function formatting() {
  return [
    bubble,
    keymap.of([
      { key: "Mod-j", run: (v) => (aiOnEditor(v), true) },
      { key: "Mod-b", run: (v) => (toggleMark(v, "**"), true) },
      { key: "Mod-i", run: (v) => (toggleMark(v, "*"), true) },
      { key: "Mod-Shift-x", run: (v) => (toggleMark(v, "~~"), true) },
      { key: "Mod-Shift-h", run: (v) => (toggleMark(v, "=="), true) },
    ]),
    EditorView.baseTheme({}),
  ];
}
