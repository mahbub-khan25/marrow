// The interface language. Instead of threading a translate() call through
// every screen, Marrow translates its own words as they appear: text and the
// title, placeholder and aria-label of elements, matched exactly against a
// dictionary. Note content is never touched: the editor, rendered notes and
// anything marked translate="no" (note names, search results…) are skipped.

import { BN, BN_PATTERNS } from "./i18n-bn";

export type Lang = "en" | "bn";

const ATTRS = ["title", "placeholder", "aria-label"] as const;
// Places that show the person's own words: note and folder names, tabs,
// search results, tasks, outlines.
const SKIP = [
  '[translate="no"]', ".cm-editor", ".preview", "script", "style", "textarea", '[contenteditable="true"]',
  ".tm-page", ".hist-body", ".tree-items", "#tabs .label", ".task-text", ".cal-title", "#recent .t", ".inspector .t", ".snip",
  ".note-title", "#outline", "#mentions", "#outgoing", "#unlinked", "#local-graph",
].join(", ");
/** Text boxes keep what's typed in them, but their placeholder and title are the app's. */
const SKIP_ATTRS = SKIP.replace(', textarea', '');

let lang: Lang = "en";
let observer: MutationObserver | null = null;
/** What each translated node said in English, to switch back. */
const texts = new Map<Text, string>();
const attrs = new Map<Element, Map<string, string>>();

const DIGITS = "০১২৩৪৫৬৭৮৯";
export const bnDigits = (s: string) => s.replace(/[0-9]/g, (d) => DIGITS[Number(d)]);

/** `s` in Bangla, or null if it isn't one of Marrow's phrases. */
export function translate(s: string): string | null {
  const key = s.trim();
  if (!key) return null;
  const hit = BN[key];
  if (hit) return s.replace(key, hit);
  for (const [re, to] of BN_PATTERNS) {
    if (re.test(key)) return s.replace(key, bnDigits(key.replace(re, to)));
  }
  return null;
}

/** The locale for dates and numbers in the interface. */
export const uiLocale = () => (lang === "bn" ? "bn-BD" : undefined);
export const currentLang = () => lang;

function skip(el: Element | null): boolean {
  return !el || !!el.closest(SKIP);
}

function doText(t: Text) {
  if (skip(t.parentElement)) return;
  const tr = translate(t.data);
  if (tr === null || tr === t.data) return;
  if (!texts.has(t)) texts.set(t, t.data);
  t.data = tr;
}

function doAttrs(el: Element) {
  if (!el || el.closest(SKIP_ATTRS)) return;
  for (const a of ATTRS) {
    const v = el.getAttribute(a);
    if (!v) continue;
    const tr = translate(v);
    if (tr === null || tr === v) continue;
    let m = attrs.get(el);
    if (!m) attrs.set(el, (m = new Map()));
    if (!m.has(a)) m.set(a, v);
    el.setAttribute(a, tr);
  }
}

function walk(root: Node) {
  if (root.nodeType === Node.TEXT_NODE) return doText(root as Text);
  if (root.nodeType !== Node.ELEMENT_NODE) return;
  const el = root as Element;
  doAttrs(el);
  if (skip(el)) return;
  const w = document.createTreeWalker(el, NodeFilter.SHOW_TEXT | NodeFilter.SHOW_ELEMENT, {
    acceptNode: (n) => {
      if (n.nodeType !== Node.ELEMENT_NODE || !(n as Element).matches(SKIP)) return NodeFilter.FILTER_ACCEPT;
      doAttrs(n as Element);
      return NodeFilter.FILTER_REJECT;
    },
  });
  for (let n = w.nextNode(); n; n = w.nextNode()) {
    if (n.nodeType === Node.TEXT_NODE) doText(n as Text);
    else doAttrs(n as Element);
  }
}

/** Switches the interface language, translating what's on screen now and from then on. */
export function setLanguage(l: Lang) {
  lang = l;
  document.documentElement.lang = l;
  observer?.disconnect();
  observer = null;
  if (l === "en") {
    for (const [t, en] of texts) if (t.isConnected) t.data = en;
    for (const [el, m] of attrs) if (el.isConnected) for (const [a, v] of m) el.setAttribute(a, v);
    texts.clear();
    attrs.clear();
    return;
  }
  walk(document.body);
  observer = new MutationObserver((records) => {
    for (const r of records) {
      if (r.type === "childList") r.addedNodes.forEach(walk);
      else if (r.type === "characterData") {
        const t = r.target as Text;
        // The app wrote new English text into a node translated before.
        if (translate(t.data) !== null) texts.delete(t);
        doText(t);
      } else if (r.type === "attributes") doAttrs(r.target as Element);
    }
    // Forget nodes that left the page.
    if (texts.size > 5000) for (const t of texts.keys()) if (!t.isConnected) texts.delete(t);
    if (attrs.size > 5000) for (const e of attrs.keys()) if (!e.isConnected) attrs.delete(e);
  });
  observer.observe(document.body, { childList: true, subtree: true, characterData: true, attributes: true, attributeFilter: [...ATTRS] });
}

/** The language to start in: the saved choice, else Bangla on a Bangla system. */
export function defaultLang(): Lang {
  return (navigator.language || "").toLowerCase().startsWith("bn") ? "bn" : "en";
}
