// Writing comfort in Source: the system spell checker (red underlines, with
// suggestions on right-click) and optional Vim keys. Both follow Settings →
// Editor and can change while a note is open.

import { Compartment, Prec, type Extension } from "@codemirror/state";
import { EditorView } from "@codemirror/view";
import { Vim, vim } from "@replit/codemirror-vim";

import { prefs } from "./prefs";

const compartment = new Compartment();

function extensions(): Extension[] {
  return [
    prefs.vim ? Prec.highest(vim({ status: true })) : [],
    EditorView.contentAttributes.of({
      spellcheck: prefs.spellcheck ? "true" : "false",
      autocorrect: "off",
      autocapitalize: "off",
    }),
  ];
}

export function writingOptions(): Extension {
  return compartment.of(extensions());
}

/** Applies the current settings to an editor (after a change, or to a kept state). */
export function applyWriting(view: EditorView) {
  view.dispatch({ effects: compartment.reconfigure(extensions()) });
}

/** ":w" in Vim mode saves the note. */
export function onVimWrite(save: () => void) {
  Vim.defineEx("write", "w", () => save());
}
