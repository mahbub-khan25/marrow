// Bangla typing: type Bangla with an English keyboard, the Avro Phonetic way
// ("ami tomake bhalobashi" → "আমি তোমাকে ভালোবাসি" as you type). Works in
// the editor and in every text box. F12 (Avro's own key) or the button in
// the status bar switches it on and off.

import { syntaxTree } from "@codemirror/language";
import { Annotation, Prec, type Extension } from "@codemirror/state";
import { EditorView, keymap } from "@codemirror/view";

import Phonetic from "./vendor/avro-phonetic.js";

/** Keys that belong to a word being typed phonetically. */
const ROMAN = /^[A-Za-z0-9`^:$.,]$/;

export function toBangla(roman: string): string {
  return Phonetic.parse(roman);
}

// ------------------------------------------------------------- on / off

let on = false;
const listeners = new Set<(on: boolean) => void>();

export const banglaTyping = () => on;

export function setBanglaTyping(v: boolean) {
  if (on === v) return;
  on = v;
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

try {
  on = localStorage.getItem("marrow.banglaTyping") === "1";
} catch {
  /* fine */
}

// --------------------------------------------------- the word being typed

/** Where the current word starts, its English letters and its Bangla. */
interface Word {
  from: number;
  roman: string;
  out: string;
}

/** The Bangla for `roman` + `key`, if `key` continues the word ending at `at`. */
export function step(word: Word | undefined, at: number, before: string, key: string): Word {
  const goesOn = word && at === word.from + word.out.length && before === word.out;
  const roman = (goesOn ? word.roman : "") + key;
  return { from: goesOn ? word.from : at, roman, out: toBangla(roman) };
}

// ---------------------------------------------------------- the editor

const ours = Annotation.define<boolean>();
const words = new WeakMap<EditorView, Word>();

function inCode(view: EditorView, pos: number): boolean {
  for (let n: ReturnType<ReturnType<typeof syntaxTree>["resolveInner"]> | null = syntaxTree(view.state).resolveInner(pos, -1); n; n = n.parent) {
    if (/Code|CodeText|CodeBlock|URL|Comment/.test(n.name)) return true;
  }
  return false;
}

export function banglaEditor(): Extension {
  return [
    EditorView.inputHandler.of((view, from, to, text) => {
      if (!on || view.composing || text.length !== 1 || !ROMAN.test(text) || inCode(view, from)) {
        words.delete(view);
        return false;
      }
      const prev = words.get(view);
      const before = prev ? view.state.sliceDoc(prev.from, from) : "";
      const w = from === to ? step(prev, from, before, text) : step(undefined, from, "", text);
      const start = w.from;
      const end = from === to ? from : to;
      view.dispatch({
        changes: { from: start, to: end, insert: w.out },
        selection: { anchor: start + w.out.length },
        userEvent: "input.type",
        annotations: ours.of(true),
        scrollIntoView: true,
      });
      words.set(view, w);
      return true;
    }),
    Prec.highest(
      keymap.of([
        {
          key: "Backspace",
          run: (view) => {
            const w = words.get(view);
            const sel = view.state.selection.main;
            if (!on || !w || !sel.empty || sel.head !== w.from + w.out.length || view.state.sliceDoc(w.from, sel.head) !== w.out) {
              words.delete(view);
              return false;
            }
            // Take back one English letter, not one Bangla sign.
            const roman = w.roman.slice(0, -1);
            const out = roman ? toBangla(roman) : "";
            view.dispatch({ changes: { from: w.from, to: sel.head, insert: out }, selection: { anchor: w.from + out.length }, userEvent: "delete.backward", annotations: ours.of(true) });
            if (roman) words.set(view, { from: w.from, roman, out });
            else words.delete(view);
            return true;
          },
        },
      ]),
    ),
  ];
}

// --------------------------------------------------- text boxes and inputs

const boxWords = new WeakMap<HTMLElement, Word>();
type Box = HTMLInputElement | HTMLTextAreaElement;

function isBox(t: EventTarget | null): t is Box {
  if (t instanceof HTMLTextAreaElement) return !t.readOnly;
  return t instanceof HTMLInputElement && ["text", "search", ""].includes(t.type) && !t.readOnly && t.dataset.noBangla === undefined;
}

/** Bangla typing in every text box (the editor has its own, above). */
export function installBanglaInputs() {
  document.addEventListener(
    "beforeinput",
    (e) => {
      const t = e.target;
      if (!on || !isBox(t) || t.closest(".cm-editor")) return;
      if (e.inputType !== "insertText" || !e.data || e.data.length !== 1 || !ROMAN.test(e.data) || e.isComposing) {
        boxWords.delete(t);
        return;
      }
      const s = t.selectionStart ?? t.value.length;
      const end = t.selectionEnd ?? s;
      const prev = boxWords.get(t);
      const w = s === end ? step(prev, s, prev ? t.value.slice(prev.from, s) : "", e.data) : step(undefined, s, "", e.data);
      e.preventDefault();
      t.setRangeText(w.out, w.from, s === end ? s : end, "end");
      boxWords.set(t, w);
      t.dispatchEvent(new Event("input", { bubbles: true }));
    },
    true,
  );
  document.addEventListener(
    "keydown",
    (e) => {
      const t = e.target;
      if (!on || e.key !== "Backspace" || !isBox(t) || t.closest(".cm-editor")) return;
      const w = boxWords.get(t);
      const s = t.selectionStart ?? 0;
      if (!w || s !== t.selectionEnd || s !== w.from + w.out.length || t.value.slice(w.from, s) !== w.out) {
        boxWords.delete(t);
        return;
      }
      e.preventDefault();
      const roman = w.roman.slice(0, -1);
      const out = roman ? toBangla(roman) : "";
      t.setRangeText(out, w.from, s, "end");
      if (roman) boxWords.set(t, { from: w.from, roman, out });
      else boxWords.delete(t);
      t.dispatchEvent(new Event("input", { bubbles: true }));
    },
    true,
  );
}
