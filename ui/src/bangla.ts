// Bangla typing with Ekushe Next: type Bangla on an English keyboard
// ("ami banglay gan gai" → "আমি বাংলায় গান গাই"), with Ekushe's candidates,
// forgiving spellings, যুক্তবর্ণ, next-word predictions and Backspace undo.
// Works in the editor and in every text box. F12 or the অ button in the
// status bar switches it on and off.
//
// The engine is Ekushe Next's own (ekushe-core built for the web, in
// public/ekushe/), loaded the first time Bangla typing is switched on.

import { syntaxTree } from "@codemirror/language";
import { Annotation, Prec, StateEffect, StateField, type Extension } from "@codemirror/state";
import { Decoration, EditorView, keymap } from "@codemirror/view";

import { Ekushe, type EkCandidate, type EkOutput } from "./vendor/ekushe.js";

/** Keys the engine reads; anything else ends the word and is typed as is. */
const ROMAN = /^[A-Za-z0-9`^:$.,]$/;
/** Candidates shown at once. */
const SHOWN = 6;

// --------------------------------------------------------------- engine

type Engine = Pick<Ekushe, "key" | "backspace" | "select" | "commit" | "reset">;
let engine: Engine | null = null;
let loading: Promise<Engine | null> | null = null;

export function loadEngine(base?: URL): Promise<Engine | null> {
  if (engine) return Promise.resolve(engine);
  loading ??= Ekushe.load(base ?? new URL("ekushe/", document.baseURI)).then(
    (e) => (engine = e),
    (err) => {
      console.warn("Ekushe Next didn't start", err);
      loading = null;
      return null;
    },
  );
  return loading;
}

/** For tests: use an engine already loaded. */
export function useEngine(e: Engine | null) {
  engine = e;
  composer.reset();
}

// ------------------------------------------------------------- on / off

let on = false;
const listeners = new Set<(on: boolean) => void>();

export const banglaTyping = () => on;

export function setBanglaTyping(v: boolean) {
  if (on === v) return;
  on = v;
  composer.reset();
  if (v) void loadEngine();
  try {
    localStorage.setItem("marrow.banglaTyping", v ? "1" : "0");
  } catch {
    /* fine */
  }
  for (const fn of listeners) fn(v);
}

export function onBanglaTyping(fn: (on: boolean) => void) {
  listeners.add(fn);
}

// ------------------------------------------------------------ composing

/** Where the word is being typed: the editor or a text box. */
export interface Host {
  /** Replaces [from, to) with `text` and puts the caret after it. */
  replace(from: number, to: number, text: string, composing: { from: number; to: number } | null): void;
  caret(): number;
  slice(from: number, to: number): string;
  /** Screen position for the candidate list. */
  rect(pos: number): { left: number; top: number; bottom: number } | null;
  key: object;
}

/**
 * One word being typed, shown in the text as Ekushe's top candidate (the
 * text the person sees is the text they get), with the candidates beside it.
 */
export class Composer {
  host: Host | null = null;
  /** Where the shown word starts, and the shown word. */
  from = 0;
  shown = "";
  roman = "";
  cands: EkCandidate[] = [];
  sel = 0;
  predictions: string[] = [];
  /** The caret right after the last word committed (Backspace there brings the English back). */
  undoAt: number | null = null;
  onChange: () => void = () => {};

  get composing() {
    return this.roman !== "";
  }

  reset() {
    if (this.composing || this.undoAt !== null) engine?.reset();
    this.host = null;
    this.roman = "";
    this.shown = "";
    this.cands = [];
    this.sel = 0;
    this.predictions = [];
    this.undoAt = null;
    this.onChange();
  }

  /** Is `host` still where we left it (no click elsewhere, no edit by someone else)? */
  private valid(host: Host): boolean {
    if (this.host?.key !== host.key) return false;
    const c = host.caret();
    if (this.composing) return c === this.from + this.shown.length && host.slice(this.from, c) === this.shown;
    return this.undoAt === c;
  }

  private begin(host: Host) {
    if (!this.valid(host)) {
      this.reset();
      this.host = host;
    }
  }

  apply(host: Host, out: EkOutput) {
    const del = out.delete_before ?? 0;
    const committed = out.committed ?? "";
    const start = this.composing ? this.from : host.caret();
    const end = this.composing ? this.from + this.shown.length : start;
    this.roman = out.roman_buffer ?? "";
    this.cands = out.candidates ?? [];
    if (!this.roman) this.sel = 0;
    this.sel = Math.min(this.sel, Math.max(0, this.cands.length - 1));
    const display = this.roman ? (this.cands[this.sel]?.text ?? out.preedit ?? "") : "";
    const at = Math.max(0, start - del);
    this.from = at + committed.length;
    this.shown = display;
    host.replace(at, end, committed + display, this.roman ? { from: this.from, to: this.from + display.length } : null);
    this.host = host;
    this.undoAt = !this.roman && committed ? host.caret() : null;
    this.predictions = !this.roman ? (out.next_word_predictions ?? []) : [];
    this.onChange();
  }

  /** A typed character. True when it was taken (don't type it again). */
  char(host: Host, ch: string): boolean {
    if (!on || !engine) return false;
    this.begin(host);
    if (ROMAN.test(ch) || (ch === "+" && this.composing)) {
      this.apply(host, engine.key(ch));
      return true;
    }
    if (ch === " " && this.composing) {
      this.apply(host, engine.select(this.sel, " "));
      return true;
    }
    // Anything else ends the word, then is typed as it is.
    if (this.composing) this.apply(host, engine.commit(""));
    this.undoAt = null;
    return false;
  }

  /** Enter: the word is finished; the line break itself is typed as usual. */
  enter(host: Host): boolean {
    if (!on || !engine) return false;
    this.begin(host);
    if (this.composing) this.apply(host, engine.select(this.sel, ""));
    this.undoAt = null;
    return false;
  }

  backspace(host: Host): boolean {
    if (!on || !engine) return false;
    this.begin(host);
    if (this.composing) {
      this.apply(host, engine.backspace());
      return true;
    }
    if (this.undoAt !== null) {
      // Right after a word: Ekushe brings back the English letters typed.
      const out = engine.backspace();
      this.undoAt = null;
      if (out.committed || out.delete_before) {
        this.apply(host, out);
        return true;
      }
    }
    return false;
  }

  /** Tab / arrows: the next or previous candidate. */
  cycle(host: Host, dir: 1 | -1): boolean {
    if (!on || !this.valid(host) || !this.composing || this.cands.length < 2) return false;
    const n = Math.min(this.cands.length, SHOWN);
    this.pick(host, (this.sel + dir + n) % n);
    return true;
  }

  /** Shows candidate `i` in the text (without finishing the word). */
  pick(host: Host, i: number) {
    const text = this.cands[i]?.text;
    if (text === undefined) return;
    this.sel = i;
    host.replace(this.from, this.from + this.shown.length, text, { from: this.from, to: this.from + text.length });
    this.shown = text;
    this.onChange();
  }

  /** Finishes the word with candidate `i` (a click in the list). */
  choose(host: Host, i: number) {
    if (!engine || !this.valid(host) || !this.composing) return;
    this.apply(host, engine.select(i, ""));
  }

  /** Escape: keep the word as shown. */
  done(host: Host): boolean {
    if (!engine || !this.valid(host) || !this.composing) return false;
    this.apply(host, engine.select(this.sel, ""));
    return true;
  }
}

export const composer = new Composer();

// ------------------------------------------------- the candidate list

let pop: HTMLElement | null = null;

function renderPop() {
  if (typeof document === "undefined") return;
  const c = composer;
  const show = on && c.host && c.composing && c.cands.length > 0;
  if (!show) {
    if (pop) pop.hidden = true;
    return;
  }
  if (!pop) {
    pop = document.createElement("div");
    pop.className = "ek-pop";
    pop.setAttribute("translate", "no");
    pop.setAttribute("role", "listbox");
    pop.addEventListener("pointerdown", (e) => {
      const li = (e.target as HTMLElement).closest<HTMLElement>("[data-i]");
      if (!li || !c.host) return;
      e.preventDefault();
      c.choose(c.host, Number(li.dataset.i));
    });
    document.body.append(pop);
  }
  const roman = document.createElement("div");
  roman.className = "ek-roman";
  roman.textContent = c.roman;
  const list = c.cands.slice(0, SHOWN).map((cand, i) => {
    const li = document.createElement("div");
    li.className = "ek-cand";
    li.dataset.i = String(i);
    li.setAttribute("role", "option");
    li.setAttribute("aria-selected", String(i === c.sel));
    const n = document.createElement("b");
    n.textContent = "১২৩৪৫৬"[i];
    li.append(n, cand.text);
    if (cand.kind === "English") {
      const en = document.createElement("small");
      en.textContent = "EN";
      li.append(en);
    }
    return li;
  });
  pop.replaceChildren(roman, ...list);
  pop.hidden = false;
  const r = c.host!.rect(c.from);
  if (r) {
    const w = pop.offsetWidth;
    const h = pop.offsetHeight;
    const left = Math.max(8, Math.min(r.left - 8, innerWidth - w - 8));
    const below = r.bottom + 6 + h < innerHeight;
    pop.style.left = `${left}px`;
    pop.style.top = `${below ? r.bottom + 6 : r.top - h - 6}px`;
  }
}

composer.onChange = renderPop;

// ---------------------------------------------------------- the editor

const ours = Annotation.define<boolean>();
const setMark = StateEffect.define<{ from: number; to: number } | null>();
const markField = StateField.define({
  create: () => Decoration.none,
  update(deco, tr) {
    deco = deco.map(tr.changes);
    for (const e of tr.effects) if (e.is(setMark)) deco = e.value && e.value.to > e.value.from ? Decoration.set([Decoration.mark({ class: "ek-composing" }).range(e.value.from, e.value.to)]) : Decoration.none;
    return deco;
  },
  provide: (f) => EditorView.decorations.from(f),
});

const hosts = new WeakMap<EditorView, Host>();
function viewHost(view: EditorView): Host {
  let host = hosts.get(view);
  if (!host) {
    host = {
      key: view,
      caret: () => view.state.selection.main.head,
      slice: (a, b) => view.state.sliceDoc(a, b),
      rect: (pos) => view.coordsAtPos(Math.min(pos, view.state.doc.length)),
      replace: (from, to, text, composing) =>
        view.dispatch({
          changes: { from, to, insert: text },
          selection: { anchor: from + text.length },
          effects: setMark.of(composing),
          userEvent: "input.type",
          annotations: ours.of(true),
          scrollIntoView: true,
        }),
    };
    hosts.set(view, host);
  }
  return host;
}

function inCode(view: EditorView, pos: number): boolean {
  for (let n: ReturnType<ReturnType<typeof syntaxTree>["resolveInner"]> | null = syntaxTree(view.state).resolveInner(pos, -1); n; n = n.parent) {
    if (/Code|CodeText|CodeBlock|URL|Comment|HTML/.test(n.name)) return true;
  }
  return false;
}

export function banglaEditor(): Extension {
  const when = (fn: (view: EditorView, host: Host) => boolean) => (view: EditorView) => on && fn(view, viewHost(view));
  return [
    markField,
    EditorView.inputHandler.of((view, from, to, text) => {
      if (!on || view.composing || text.length !== 1 || inCode(view, from)) return false;
      const host = viewHost(view);
      if (from !== to) {
        // Typing over a selection: remove it first, then type into the gap.
        view.dispatch({ changes: { from, to }, selection: { anchor: from }, annotations: ours.of(true) });
      }
      return composer.char(host, text);
    }),
    // A click elsewhere or someone else's edit ends the word where it is.
    EditorView.updateListener.of((u) => {
      if (u.transactions.some((t) => t.annotation(ours))) return;
      if ((u.docChanged || u.selectionSet) && composer.host?.key === u.view) {
        if (composer.composing) u.view.dispatch({ effects: setMark.of(null) });
        composer.reset();
      }
    }),
    EditorView.domEventHandlers({
      blur: (_e, view) => {
        if (composer.host?.key === view) {
          if (composer.composing) view.dispatch({ effects: setMark.of(null) });
          composer.reset();
        }
      },
    }),
    Prec.highest(
      keymap.of([
        { key: "Backspace", run: when((_v, h) => composer.backspace(h)) },
        { key: "Enter", run: when((_v, h) => composer.enter(h)) },
        { key: "Tab", run: when((_v, h) => composer.cycle(h, 1)) },
        { key: "ArrowDown", run: when((_v, h) => composer.cycle(h, 1)) },
        { key: "ArrowUp", run: when((_v, h) => composer.cycle(h, -1)) },
        { key: "Escape", run: when((_v, h) => composer.done(h)) },
      ]),
    ),
  ];
}

// --------------------------------------------------- text boxes and inputs

type Box = HTMLInputElement | HTMLTextAreaElement;
const boxHosts = new WeakMap<Box, Host>();

function isBox(t: EventTarget | null): t is Box {
  if (t instanceof HTMLTextAreaElement) return !t.readOnly && !t.closest(".cm-editor");
  return t instanceof HTMLInputElement && ["text", "search", ""].includes(t.type) && !t.readOnly && t.dataset.noBangla === undefined;
}

function boxHost(el: Box): Host {
  let host = boxHosts.get(el);
  if (!host) {
    host = {
      key: el,
      caret: () => el.selectionStart ?? el.value.length,
      slice: (a, b) => el.value.slice(a, b),
      rect: () => {
        const r = el.getBoundingClientRect();
        return { left: r.left + 8, top: r.top, bottom: el instanceof HTMLTextAreaElement ? Math.min(r.bottom, r.top + 40) : r.bottom };
      },
      replace: (from, to, text) => {
        el.setRangeText(text, from, to, "end");
        el.dispatchEvent(new Event("input", { bubbles: true }));
      },
    };
    boxHosts.set(el, host);
  }
  return host;
}

/** Bangla typing in every text box (the editor has its own, above). */
export function installBanglaInputs() {
  document.addEventListener(
    "beforeinput",
    (e) => {
      const t = e.target;
      if (!on || !isBox(t) || e.isComposing) return;
      const host = boxHost(t);
      if (e.inputType === "insertText" && e.data && e.data.length === 1) {
        if ((t.selectionStart ?? 0) !== (t.selectionEnd ?? 0)) t.setRangeText("", t.selectionStart ?? 0, t.selectionEnd ?? 0, "end");
        if (composer.char(host, e.data)) e.preventDefault();
      } else if (e.inputType === "deleteContentBackward") {
        if (composer.backspace(host)) e.preventDefault();
      } else if (e.inputType === "insertLineBreak") composer.enter(host);
    },
    true,
  );
  document.addEventListener(
    "keydown",
    (e) => {
      const t = e.target;
      if (!on || !isBox(t) || !composer.composing) return;
      const host = boxHost(t);
      // Enter in a single-line box usually submits: finish the word first.
      if (e.key === "Enter") composer.enter(host);
      else if ((e.key === "Tab" || e.key === "ArrowDown") && composer.cycle(host, 1)) e.preventDefault();
      else if (e.key === "ArrowUp" && composer.cycle(host, -1)) e.preventDefault();
      else if (e.key === "Escape" && composer.done(host)) {
        e.preventDefault();
        e.stopPropagation();
      }
    },
    true,
  );
  for (const type of ["focusout", "pointerdown"]) {
    document.addEventListener(
      type,
      (e) => {
        if (pop?.contains(e.target as Node)) return;
        if (composer.host && composer.host.key instanceof HTMLElement && !(composer.host.key as HTMLElement).contains(e.target as Node)) composer.reset();
        else if (type === "pointerdown" && composer.host?.key instanceof HTMLElement) composer.reset();
      },
      true,
    );
  }
}

try {
  if (localStorage.getItem("marrow.banglaTyping") === "1") setBanglaTyping(true);
} catch {
  /* fine */
}
