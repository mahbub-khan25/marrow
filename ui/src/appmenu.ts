// The menus in the desktop's global menu bar (Linux on Wayland, where the
// desktop has one: Hyperlume's top bar, KDE Plasma's global menu). The window
// itself has no menu bar. The menu is defined in Rust
// (crates/marrow-appmenu/src/model.rs); here the window runs what's clicked,
// the way its shortcut or button would, and tells the menu its labels in the
// interface language, which toggles are on and what can be used just now.

import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { redo, selectAll, undo } from "@codemirror/commands";
import { openSearchPanel } from "@codemirror/search";
import { EditorView } from "@codemirror/view";
import { currentLang, translate } from "./i18n";

/** What each menu item does, by the action's name (`Action::name` in Rust). */
export type MenuActions = Record<string, () => unknown>;

/** Toggles that are on, and items that can't be used now (false), by name. */
export interface MenuState {
  checked: Record<string, boolean>;
  enabled: Record<string, boolean>;
}

let stateOf: (() => MenuState) | null = null;
/** Each menu and item's name and English label, as the menu gave them. */
let english: [string, string][] = [];
let lastSent = "";
let queued = false;

/** Starts listening to the global menu. Only on Linux: elsewhere there is none. */
export async function initAppMenu(actions: MenuActions, state: () => MenuState): Promise<void> {
  stateOf = state;
  await listen<string>("app-menu", (e) => runMenuAction(actions, e.payload));
  syncAppMenu();
}

export function runMenuAction(actions: MenuActions, name: string): boolean {
  const run = actions[name];
  if (!run) return false;
  void run();
  return true;
}

/** Something the menu shows may have changed: tell it (batched, and only what changed). */
export function syncAppMenu() {
  if (!stateOf || queued) return;
  queued = true;
  queueMicrotask(() => {
    queued = false;
    void send();
  });
}

/** The menu's labels in the interface language; none in English. */
export function menuLabels(en: [string, string][], lang: string): Record<string, string> {
  const out: Record<string, string> = {};
  if (lang === "en") return out;
  for (const [key, label] of en) {
    const t = translate(label);
    if (t !== null && t !== label) out[key] = t;
  }
  return out;
}

async function send() {
  if (!stateOf) return;
  const { checked, enabled } = stateOf();
  const labels = menuLabels(english, currentLang());
  const body = JSON.stringify({ labels, checked, enabled });
  if (body === lastSent) return;
  lastSent = body;
  try {
    const en = await invoke<[string, string][]>("app_menu_sync", { labels, checked, enabled });
    // The first answer brings the English labels: now they can be translated.
    if (!english.length && en.length) {
      english = en;
      if (currentLang() !== "en") syncAppMenu();
    }
  } catch {
    // No global menu in this build: stop asking.
    stateOf = null;
  }
}

/** Quits from the menu (the window has saved everything first). */
export function quitApp(): Promise<void> {
  return invoke("app_menu_quit");
}

/** The editor that has the focus (or holds it, the focus being in its search box). */
function focusedEditor(): EditorView | null {
  const el = document.activeElement;
  const root = el instanceof HTMLElement ? el.closest<HTMLElement>(".cm-editor") : null;
  return root ? EditorView.findFromDOM(root) : null;
}

/** A text box or text area (not an editor) with the focus. */
function inTextField(): boolean {
  const el = document.activeElement;
  return el instanceof HTMLInputElement || el instanceof HTMLTextAreaElement || (el instanceof HTMLElement && el.isContentEditable && !el.closest(".cm-editor"));
}

/**
 * Undo, Redo, Select all and Find from the menu, on what has the focus: the
 * editor's own commands in a note (its history, not the browser's), the
 * browser's in a text box. With the focus elsewhere, the note being written
 * (`fallback`) if there is one. False if there was nothing to act on.
 */
export function editCommand(kind: "undo" | "redo" | "select-all" | "find", fallback: () => EditorView | null): boolean {
  const view = focusedEditor() ?? (inTextField() ? null : fallback());
  if (view) {
    view.focus();
    return { undo, redo, "select-all": selectAll, find: openSearchPanel }[kind](view);
  }
  if (kind === "find" || !inTextField()) return false;
  return document.execCommand(kind === "select-all" ? "selectAll" : kind);
}
