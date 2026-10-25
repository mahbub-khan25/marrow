// CodeMirror 6 setup: Markdown, wikilink highlighting, Ctrl-click to follow,
// and `[[` autocomplete.

import { banglaEditor } from "./bangla";
import {
  autocompletion,
  closeBrackets,
  closeBracketsKeymap,
  completionKeymap,
  type Completion,
  type CompletionContext,
  type CompletionResult,
} from "@codemirror/autocomplete";
import { defaultKeymap, history, historyKeymap, indentWithTab } from "@codemirror/commands";
import { markdown, markdownLanguage } from "@codemirror/lang-markdown";
import {
  codeFolding,
  foldEffect,
  foldGutter,
  foldKeymap,
  HighlightStyle,
  indentOnInput,
  syntaxHighlighting,
  syntaxTree,
  unfoldEffect,
} from "@codemirror/language";
import type { SyntaxNode } from "@lezer/common";
import { openUrl } from "@tauri-apps/plugin-opener";
import { languages } from "@codemirror/language-data";
import { highlightSelectionMatches, search, searchKeymap } from "@codemirror/search";
import { EditorState, StateEffect, StateField, type Extension } from "@codemirror/state";
import {
  Decoration,
  type DecorationSet,
  drawSelection,
  EditorView,
  keymap,
  MatchDecorator,
  placeholder,
  tooltips,
  ViewPlugin,
  type ViewUpdate,
} from "@codemirror/view";
import { tags as t } from "@lezer/highlight";

import { api } from "./api";
import { formatting } from "./format";
import { writingOptions } from "./writing";
import { livePreview } from "./livepreview";
import { prefs } from "./prefs";
import { slashCompletions, slashIcon, type SlashHooks } from "./slash";

/** `[[target#sub|alias]]` and `![[...]]` on one line. */
const WIKILINK = /!?\[\[([^\[\]\n]+?)\]\]/g;

/** Link target without `#subpath` or `|alias`. */
export function linkTarget(inner: string): string {
  return inner.split("|")[0].replace(/\\$/, "").split("#")[0].trim();
}

// ---- unresolved-link state: lower-cased link texts that point nowhere ----

const setUnresolved = StateEffect.define<Set<string>>();

const unresolvedField = StateField.define<Set<string>>({
  create: () => new Set(),
  update(value, tr) {
    for (const e of tr.effects) if (e.is(setUnresolved)) return e.value;
    return value;
  },
});

export function updateUnresolved(view: EditorView, names: string[]) {
  view.dispatch({ effects: setUnresolved.of(new Set(names.map((n) => n.toLowerCase()))) });
}

const linkMatcher = new MatchDecorator({
  regexp: WIKILINK,
  decoration: (m, view) => {
    const target = linkTarget(m[1]).replace(/\.md$/i, "").toLowerCase();
    const unresolved = target !== "" && view.state.field(unresolvedField).has(target);
    return Decoration.mark({ class: unresolved ? "cm-wikilink cm-wikilink-unresolved" : "cm-wikilink" });
  },
});

const wikilinkPlugin = ViewPlugin.fromClass(
  class {
    decorations: DecorationSet;
    constructor(view: EditorView) {
      this.decorations = linkMatcher.createDeco(view);
    }
    update(u: ViewUpdate) {
      const refreshed = u.transactions.some((tr) => tr.effects.some((e) => e.is(setUnresolved)));
      this.decorations = refreshed ? linkMatcher.createDeco(u.view) : linkMatcher.updateDeco(u, this.decorations);
    }
  },
  { decorations: (v) => v.decorations },
);

/** `#tag` after a space or line start; not all digits (so `#123` isn't a tag). */
const TAG = /(^|[\s(\[,;])(#(?![\d]+(?![\p{L}\p{N}_\-\/]))[\p{L}\p{N}_\-\/]+)/gu;

const tagMatcher = new MatchDecorator({
  regexp: TAG,
  decorate: (add, from, to, m) => {
    add(from + m[1].length, to, Decoration.mark({ class: "cm-tag" }));
  },
});

const tagPlugin = ViewPlugin.fromClass(
  class {
    decorations: DecorationSet;
    constructor(view: EditorView) {
      this.decorations = tagMatcher.createDeco(view);
    }
    update(u: ViewUpdate) {
      this.decorations = tagMatcher.updateDeco(u, this.decorations);
    }
  },
  { decorations: (v) => v.decorations },
);

/** Finds the `#tag` covering `pos`, if any. */
function tagAt(state: EditorState, pos: number): string | null {
  const line = state.doc.lineAt(pos);
  TAG.lastIndex = 0;
  for (let m: RegExpExecArray | null; (m = TAG.exec(line.text)); ) {
    const from = line.from + m.index + m[1].length;
    if (pos >= from && pos <= from + m[2].length) return m[2];
  }
  return null;
}

/** Finds the wikilink covering `pos`, if any. */
export function wikilinkAt(state: EditorState, pos: number): string | null {
  const line = state.doc.lineAt(pos);
  WIKILINK.lastIndex = 0;
  for (let m: RegExpExecArray | null; (m = WIKILINK.exec(line.text)); ) {
    const from = line.from + m.index;
    if (pos >= from && pos <= from + m[0].length) return m[1];
  }
  return null;
}

/** The URL of the `[text](url)` link covering `pos`, if any. */
function markdownLinkAt(state: EditorState, pos: number): string | null {
  for (let n: SyntaxNode | null = syntaxTree(state).resolveInner(pos, 1); n; n = n.parent) {
    if (n.name === "Link" || n.name === "Image") {
      const u = n.getChild("URL");
      return u ? state.sliceDoc(u.from, u.to) : null;
    }
  }
  return null;
}

// ------------------------------------------------------------ completion

function wikiCompletions(getSource: () => string | null) {
  return async (ctx: CompletionContext): Promise<CompletionResult | null> => {
    const m = ctx.matchBefore(/\[\[[^\[\]\n|#]*$/);
    if (!m) return null;
    const query = m.text.slice(2);
    const hits = await api.findFiles(query, getSource() ?? undefined, 40);
    const options: Completion[] = hits.map((h) => ({
      label: h.name,
      detail: h.folder || undefined,
      type: h.isNote ? "text" : "file",
      apply: (view, _c, from, to) => {
        const after = view.state.sliceDoc(to, to + 2);
        const insert = h.linkText + (after === "]]" ? "" : "]]");
        const end = from + h.linkText.length + 2;
        view.dispatch({ changes: { from, to, insert }, selection: { anchor: end } });
      },
    }));
    return { from: m.from + 2, options, filter: false };
  };
}

// ----------------------------------------------------------------- theme

const highlight = HighlightStyle.define([
  { tag: t.heading1, class: "md-h md-h1" },
  { tag: t.heading2, class: "md-h md-h2" },
  { tag: t.heading3, class: "md-h md-h3" },
  { tag: [t.heading4, t.heading5, t.heading6], class: "md-h md-h4" },
  { tag: t.strong, fontWeight: "700" },
  { tag: t.emphasis, fontStyle: "italic" },
  { tag: t.strikethrough, textDecoration: "line-through" },
  { tag: t.link, class: "md-link" },
  { tag: t.url, class: "md-url" },
  { tag: t.monospace, class: "md-code" },
  { tag: t.quote, class: "md-quote" },
  { tag: [t.processingInstruction, t.meta, t.contentSeparator, t.atom], class: "md-mark" },
  { tag: [t.keyword, t.typeName], class: "tok-kw" },
  { tag: [t.string, t.special(t.string)], class: "tok-str" },
  { tag: [t.number, t.bool], class: "tok-num" },
  { tag: t.comment, class: "tok-com" },
]);

/** Layout and type for the writing column. Lives in a CodeMirror theme so
 * it outranks CodeMirror's own base styles. The editor grows with its text;
 * the pane around it scrolls. */
const writingTheme = EditorView.theme({
  "&": { height: "auto", backgroundColor: "transparent", color: "var(--color-text)" },
  "&.cm-focused": { outline: "none" },
  ".cm-scroller": {
    fontFamily: "var(--font-body)",
    lineHeight: "1.75",
    overflow: "visible",
  },
  ".cm-content": { padding: "0 0 30vh", caretColor: "var(--color-accent)" },
  ".cm-line": { padding: "0" },
  ".cm-placeholder": { color: "var(--ink-65)", fontStyle: "italic" },
});

/** Ctrl+D, Ctrl+G and Ctrl+Shift+G belong to the app (daily note, graph). */
const appKeys = new Set(["Mod-d", "Mod-g", "Shift-Mod-g", "Mod-Shift-g"]);
const editorSearchKeymap = searchKeymap.filter((b) => !appKeys.has(b.key ?? ""));

export interface EditorHooks {
  onChange: () => void;
  /** `inSplit` when Ctrl+Shift-clicked or Ctrl+Shift+Enter. */
  onFollowLink: (target: string, inSplit?: boolean) => void;
  /** Ctrl-click on a #tag. */
  onTag: (tag: string) => void;
  /** The cursor moved without an edit. */
  onCursor?: () => void;
  /** The selection or text changed (the toolbar shows the line's style). */
  onSelection?: (state: EditorState) => void;
  currentPath: () => string | null;
  /** Templates for the / menu. */
  slash?: SlashHooks;
  /** A section was folded or unfolded. */
  onFolds?: () => void;
}

/** Fold arrows sit in the left margin, out of the writing column. */
function foldMarker(open: boolean): HTMLElement {
  const el = document.createElement("span");
  el.className = `cm-fold-marker${open ? "" : " is-folded"}`;
  el.innerHTML = `<i class="ph-duotone ph-caret-${open ? "down" : "right"}"></i>`;
  el.title = open ? "Fold" : "Unfold";
  return el;
}

export function editorExtensions(hooks: EditorHooks): Extension[] {
  return [
    banglaEditor(),
    writingOptions(),
    history(),
    drawSelection(),
    indentOnInput(),
    closeBrackets(),
    highlightSelectionMatches(),
    search({ top: true }),
    EditorView.lineWrapping,
    EditorState.allowMultipleSelections.of(true),
    placeholder("Start writing. Type [[ to link a note, # to tag it."),
    markdown({ base: markdownLanguage, codeLanguages: languages }),
    syntaxHighlighting(highlight),
    writingTheme,
    livePreview(prefs.livePreview),
    unresolvedField,
    wikilinkPlugin,
    tagPlugin,
    autocompletion({
      override: [wikiCompletions(hooks.currentPath), ...(hooks.slash ? [slashCompletions(hooks.slash)] : [])],
      icons: false,
      addToOptions: [slashIcon],
    }),
    // Popups live at the top of the page: the note column's container sizing
    // would otherwise shift fixed-position popups off-screen.
    tooltips({ parent: document.body, position: "fixed" }),
    formatting(),
    codeFolding({ placeholderText: "…" }),
    foldGutter({ markerDOM: foldMarker }),
    keymap.of([
      ...closeBracketsKeymap,
      ...completionKeymap,
      ...editorSearchKeymap,
      ...historyKeymap,
      indentWithTab,
      ...foldKeymap,
      ...defaultKeymap,
      {
        key: "Mod-Enter",
        run: (view) => {
          const link = wikilinkAt(view.state, view.state.selection.main.head);
          if (!link) return false;
          hooks.onFollowLink(linkTarget(link));
          return true;
        },
      },
    ]),
    EditorView.domEventHandlers({
      mousedown(e, view) {
        if (!(e.ctrlKey || e.metaKey) || e.button !== 0) return false;
        const pos = view.posAtCoords({ x: e.clientX, y: e.clientY });
        if (pos == null) return false;
        const link = wikilinkAt(view.state, pos);
        if (link) {
          e.preventDefault();
          hooks.onFollowLink(linkTarget(link), e.shiftKey);
          return true;
        }
        const tag = tagAt(view.state, pos);
        if (tag) {
          e.preventDefault();
          hooks.onTag(tag);
          return true;
        }
        const url = markdownLinkAt(view.state, pos);
        if (url) {
          e.preventDefault();
          if (/^[a-z][a-z0-9+.-]*:/i.test(url)) void openUrl(url).catch(() => {});
          else {
            let target = url;
            try {
              target = decodeURI(url);
            } catch {
              /* keep as written */
            }
            hooks.onFollowLink(linkTarget(target), e.shiftKey);
          }
          return true;
        }
        return false;
      },
    }),
    EditorView.updateListener.of((u) => {
      if (u.selectionSet && !u.docChanged) hooks.onCursor?.();
      if (u.selectionSet || u.docChanged) hooks.onSelection?.(u.state);
      if (u.docChanged) hooks.onChange();
      if (u.transactions.some((tr) => tr.effects.some((e) => e.is(foldEffect) || e.is(unfoldEffect)))) hooks.onFolds?.();
    }),
  ];
}

export function createEditor(parent: HTMLElement, hooks: EditorHooks): EditorView {
  return new EditorView({ parent, state: EditorState.create({ doc: "", extensions: editorExtensions(hooks) }) });
}

/** A fresh editor state for a note (its own undo history). */
export function makeState(text: string, hooks: EditorHooks): EditorState {
  return EditorState.create({ doc: text, extensions: editorExtensions(hooks) });
}

/** Replaces the whole document with a fresh state (clears undo history). */
export function loadDoc(view: EditorView, text: string, hooks: EditorHooks) {
  view.setState(makeState(text, hooks));
}

/**
 * Changes the document to `text` with the smallest single edit (common start
 * and end kept), so the cursor, selection and scroll stay where they were
 * unless the change is right on top of them.
 */
export function applyText(view: EditorView, text: string) {
  const old = view.state.doc.toString();
  if (old === text) return;
  let start = 0;
  const max = Math.min(old.length, text.length);
  while (start < max && old.charCodeAt(start) === text.charCodeAt(start)) start++;
  let endOld = old.length;
  let endNew = text.length;
  while (endOld > start && endNew > start && old.charCodeAt(endOld - 1) === text.charCodeAt(endNew - 1)) {
    endOld--;
    endNew--;
  }
  view.dispatch({ changes: { from: start, to: endOld, insert: text.slice(start, endNew) } });
}

/**
 * Moves the cursor to a line and character (UTF-16), optionally selecting
 * `len` characters, and scrolls it into view.
 */
export function revealPos(view: EditorView, line: number, ch: number, len = 0) {
  const l = view.state.doc.line(Math.min(line + 1, view.state.doc.lines));
  const pos = Math.min(l.from + ch, l.to);
  const head = Math.min(pos + len, l.to);
  view.dispatch({ selection: { anchor: pos, head }, effects: EditorView.scrollIntoView(pos, { y: "center" }) });
  view.focus();
}
