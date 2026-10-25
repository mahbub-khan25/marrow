// App-wide preferences (not tied to a vault), kept in local storage.

import { defaultLang, type Lang } from "./i18n";
import { storage, store } from "./util";

export type Mode = "preview" | "source";

export interface Prefs {
  /** How notes open. */
  defaultMode: Mode;
  width: "narrow" | "normal" | "wide";
  size: 16 | 18 | 20 | 22;
  theme: "system" | "light" | "dark";
  inspector: boolean;
  linkStyle: "underline" | "tint";
  /** Hide Markdown marks in Source except on the line being edited. */
  livePreview: boolean;
  /** The formatting toolbar above the Source editor. */
  toolbar: boolean;
  /** Words to aim for in focus mode (0: no goal). */
  wordGoal: 0 | 250 | 500 | 1000 | 2000;
  /** System-wide quick-capture shortcut ("" = off). */
  captureKey: "" | "CommandOrControl+Alt+N" | "CommandOrControl+Alt+Space" | "CommandOrControl+Shift+Space";
  /** Lock unlocked notes again after this many idle minutes (0: when Marrow closes). */
  lockAfter: 0 | 5 | 15 | 60;
  /** Underline misspelt words in Source (the system's dictionaries). */
  spellcheck: boolean;
  /** Vim keys in Source. */
  vim: boolean;
  /** Old notes, "on this day" and the weekly review on the start screen. */
  resurface: boolean;
  /** The interface language. */
  language: Lang;
}

const DEFAULTS: Prefs = {
  defaultMode: "preview",
  width: "normal",
  size: 18,
  theme: "system",
  inspector: true,
  linkStyle: "underline",
  livePreview: true,
  toolbar: true,
  wordGoal: 0,
  captureKey: "CommandOrControl+Alt+N",
  lockAfter: 15,
  spellcheck: true,
  vim: false,
  resurface: true,
  language: defaultLang(),
};

export const prefs: Prefs = { ...DEFAULTS, ...storage<Partial<Prefs>>("marrow.prefs", {}) };

const listeners = new Set<() => void>();

export function onPrefs(fn: () => void) {
  listeners.add(fn);
}

export function setPref<K extends keyof Prefs>(k: K, v: Prefs[K]) {
  prefs[k] = v;
  store("marrow.prefs", prefs);
  applyPrefs();
  for (const fn of listeners) fn();
}

export function applyPrefs() {
  const root = document.documentElement;
  if (prefs.theme === "system") delete root.dataset.theme;
  else root.dataset.theme = prefs.theme;
  root.style.setProperty("--note-size", `${prefs.size}px`);
  root.style.setProperty("--note-width", { narrow: "680px", normal: "820px", wide: "1080px" }[prefs.width]);
  document.body.classList.toggle("link-tint", prefs.linkStyle === "tint");
  root.dataset.toolbar = prefs.toolbar ? "on" : "off";
}
