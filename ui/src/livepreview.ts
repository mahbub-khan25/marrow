// Live preview for Source mode: Markdown marks (#, **, `, [[ ]], > , - [ ])
// are hidden and drawn as what they mean, except on the lines the cursor is
// on, where the raw text shows so it can be edited. Like Obsidian's.

import { syntaxTree } from "@codemirror/language";
import { Compartment, type EditorState, type Extension, type Range } from "@codemirror/state";
import { Decoration, type DecorationSet, EditorView, ViewPlugin, type ViewUpdate, WidgetType } from "@codemirror/view";

class BulletWidget extends WidgetType {
  eq() {
    return true;
  }
  toDOM() {
    const s = document.createElement("span");
    s.className = "cm-lp-bullet";
    s.textContent = "•";
    return s;
  }
}

class CheckboxWidget extends WidgetType {
  constructor(readonly done: boolean) {
    super();
  }
  eq(o: CheckboxWidget) {
    return o.done === this.done;
  }
  toDOM() {
    const s = document.createElement("span");
    s.className = `cm-lp-check${this.done ? " is-done" : ""}`;
    s.setAttribute("role", "checkbox");
    s.setAttribute("aria-checked", String(this.done));
    s.textContent = this.done ? "✓" : "";
    return s;
  }
  ignoreEvent() {
    return false;
  }
}

class RuleWidget extends WidgetType {
  eq() {
    return true;
  }
  toDOM() {
    const s = document.createElement("span");
    s.className = "cm-lp-hr";
    return s;
  }
}

const hide = Decoration.replace({});
const bullet = Decoration.replace({ widget: new BulletWidget() });
const rule = Decoration.replace({ widget: new RuleWidget() });
const linkText = Decoration.mark({ class: "cm-lp-link" });
const quoteLine = Decoration.line({ class: "cm-lp-quote" });
const doneLine = Decoration.line({ class: "cm-lp-done" });

const WIKILINK = /!?\[\[([^\[\]\n]+?)\]\]/g;
const HIGHLIGHT = /==([^=\n]+)==/g;
const highlight = Decoration.mark({ class: "cm-lp-mark" });
const fmLine = Decoration.line({ class: "cm-lp-fm" });

/** The YAML block at the top, if any: [from, to). The Markdown grammar doesn't know it. */
function frontmatter(state: EditorState): [number, number] | null {
  const doc = state.doc;
  if (doc.lines < 2 || doc.line(1).text.trim() !== "---") return null;
  for (let n = 2; n <= Math.min(doc.lines, 400); n++) {
    const t = doc.line(n).text.trim();
    if (t === "---" || t === "...") return [0, doc.line(n).to];
  }
  return null;
}

/** Line numbers the cursor or a selection touches (only while focused). */
function activeLines(view: EditorView): Set<number> {
  const out = new Set<number>();
  if (!view.hasFocus) return out;
  const doc = view.state.doc;
  for (const r of view.state.selection.ranges) {
    const a = doc.lineAt(r.from).number;
    const b = doc.lineAt(r.to).number;
    for (let n = a; n <= b; n++) out.add(n);
  }
  return out;
}

function build(view: EditorView): DecorationSet {
  const state = view.state;
  const doc = state.doc;
  const active = activeLines(view);
  const isActive = (pos: number) => active.has(doc.lineAt(pos).number);
  const decos: Range<Decoration>[] = [];
  const fm = frontmatter(state);
  if (fm) {
    // Plain, quiet text: not a rule and a heading.
    for (let n = 1; n <= doc.lineAt(fm[1]).number; n++) decos.push(fmLine.range(doc.line(n).from));
  }
  const inFm = (pos: number) => fm !== null && pos <= fm[1];

  for (const { from, to } of view.visibleRanges) {
    syntaxTree(state).iterate({
      from,
      to,
      enter: (node) => {
        const name = node.name;
        if (inFm(node.from)) return;
        // Code blocks and frontmatter keep their marks.
        if (name === "FencedCode" || name === "CodeBlock" || name === "Frontmatter") return false;
        if (name === "TaskMarker") {
          // Checkboxes stay clickable even on the line being edited.
          const text = doc.sliceString(node.from, node.to);
          const done = /\[[xX]\]/.test(text);
          decos.push(Decoration.replace({ widget: new CheckboxWidget(done) }).range(node.from, node.to));
          if (done) decos.push(doneLine.range(doc.lineAt(node.from).from));
          return;
        }
        if (isActive(node.from)) return;
        switch (name) {
          case "HeaderMark": {
            // "## " — the marks and the space after them.
            const line = doc.lineAt(node.from);
            const end = Math.min(node.to + 1, line.to);
            if (node.from === line.from || doc.sliceString(line.from, node.from).trim() === "") decos.push(hide.range(node.from, end));
            else decos.push(hide.range(node.from, node.to)); // closing ###
            break;
          }
          case "EmphasisMark":
          case "StrikethroughMark":
            decos.push(hide.range(node.from, node.to));
            break;
          case "CodeMark": {
            const parent = node.node.parent;
            if (parent?.name === "InlineCode") decos.push(hide.range(node.from, node.to));
            break;
          }
          case "QuoteMark": {
            const line = doc.lineAt(node.from);
            const end = doc.sliceString(node.to, node.to + 1) === " " ? node.to + 1 : node.to;
            decos.push(hide.range(node.from, end));
            decos.push(quoteLine.range(line.from));
            break;
          }
          case "ListMark": {
            const mark = doc.sliceString(node.from, node.to);
            const task = node.node.nextSibling?.name === "Task";
            if (/^[-*+]$/.test(mark) && !task) decos.push(bullet.range(node.from, node.to));
            else if (task && /^[-*+]$/.test(mark)) {
              // "- [ ]" shows only the checkbox.
              const end = doc.sliceString(node.to, node.to + 1) === " " ? node.to + 1 : node.to;
              decos.push(hide.range(node.from, end));
            }
            break;
          }
          case "HorizontalRule":
            decos.push(rule.range(node.from, node.to));
            break;
          case "Link": {
            // [text](url) → text. Leave reference links and autolinks alone.
            const marks = node.node.getChildren("LinkMark");
            const url = node.node.getChild("URL");
            if (marks.length >= 2 && url) {
              const textFrom = marks[0].to;
              const textTo = marks[1].from;
              if (textTo > textFrom) {
                decos.push(hide.range(node.from, textFrom));
                decos.push(linkText.range(textFrom, textTo));
                decos.push(hide.range(textTo, node.to));
              }
            }
            return false;
          }
          case "Image":
            return false;
        }
      },
    });

    // Wikilinks aren't in the Markdown grammar: [[target|alias]] → alias.
    const first = doc.lineAt(from).number;
    const last = doc.lineAt(to).number;
    for (let n = first; n <= last; n++) {
      if (active.has(n)) continue;
      const line = doc.line(n);
      if (inFm(line.from)) continue;
      HIGHLIGHT.lastIndex = 0;
      for (let m: RegExpExecArray | null; (m = HIGHLIGHT.exec(line.text)); ) {
        const a = line.from + m.index;
        const b = a + m[0].length;
        if (inCode(state, a)) continue;
        decos.push(hide.range(a, a + 2), highlight.range(a + 2, b - 2), hide.range(b - 2, b));
      }
      WIKILINK.lastIndex = 0;
      for (let m: RegExpExecArray | null; (m = WIKILINK.exec(line.text)); ) {
        const start = line.from + m.index;
        if (inCode(state, start)) continue;
        const inner = m[1];
        const open = start + (m[0].startsWith("!") ? 3 : 2);
        const close = start + m[0].length - 2;
        const bar = inner.indexOf("|");
        decos.push(hide.range(start, bar >= 0 ? open + bar + 1 : open));
        decos.push(hide.range(close, close + 2));
      }
    }
  }
  return Decoration.set(decos, true);
}

function inCode(state: EditorState, pos: number): boolean {
  for (let n: ReturnType<ReturnType<typeof syntaxTree>["resolveInner"]> | null = syntaxTree(state).resolveInner(pos, 1); n; n = n.parent) {
    if (n.name === "InlineCode" || n.name === "FencedCode" || n.name === "CodeBlock") return true;
  }
  return false;
}

const plugin = ViewPlugin.fromClass(
  class {
    decorations: DecorationSet;
    constructor(view: EditorView) {
      this.decorations = build(view);
    }
    update(u: ViewUpdate) {
      if (u.docChanged || u.viewportChanged || u.selectionSet || u.focusChanged || syntaxTree(u.startState) !== syntaxTree(u.state)) {
        this.decorations = build(u.view);
      }
    }
  },
  {
    decorations: (v) => v.decorations,
    eventHandlers: {
      mousedown(e, view) {
        const t = e.target as HTMLElement;
        if (!t.classList.contains("cm-lp-check")) return false;
        const pos = view.posAtDOM(t);
        const text = view.state.sliceDoc(pos, pos + 3);
        if (!/^\[[ xX]\]$/.test(text)) return false;
        e.preventDefault();
        view.dispatch({ changes: { from: pos + 1, to: pos + 2, insert: text[1] === " " ? "x" : " " } });
        return true;
      },
    },
  },
);

/** Hidden marks need the cursor to step over them in one keypress. */
const atomic = EditorView.atomicRanges.of((view) => view.plugin(plugin)?.decorations ?? Decoration.none);

export const livePreviewCompartment = new Compartment();

export function livePreview(on: boolean): Extension {
  return livePreviewCompartment.of(on ? [plugin, atomic] : []);
}

export function setLivePreview(view: EditorView, on: boolean) {
  view.dispatch({ effects: livePreviewCompartment.reconfigure(on ? [plugin, atomic] : []) });
}
