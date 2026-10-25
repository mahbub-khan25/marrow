// Command palette (Ctrl+K): notes by name, words inside notes, #tags and
// commands, all in one list. Enter opens, Ctrl+Enter opens in the split
// pane, Shift+Enter creates a note with what was typed.

import { api, displayName, isCanvasPath, parentOf } from "./api";
import { appendHighlighted } from "./sidebar";
import { $, h, icon, isMac } from "./util";

export interface Command {
  label: string;
  hint?: string;
  icon: string;
  run: () => void;
}

export type OpenHow = "replace" | "tab" | "split";

export interface PaletteApp {
  commands(): Command[];
  /** Recently opened, newest first. */
  recent(): string[];
  source(): string | undefined;
  open(path: string, how: OpenHow, at?: { line: number; ch: number; len?: number }): void;
  create(name: string, how: OpenHow): void;
}

interface Item {
  kind: "note" | "cmd" | "new" | "tag";
  label: string;
  hint: string;
  icon: string;
  path?: string;
  snip?: { text: string; ranges: [number, number][] };
  at?: { line: number; ch: number; len?: number };
  run: (how: OpenHow) => void;
}

let app: PaletteApp;
let items: Item[] = [];
let sel = 0;
let seq = 0;
let newTab = false;
const wrap = () => $("palette");
const input = () => $<HTMLInputElement>("palette-input");

export function initPalette(a: PaletteApp) {
  app = a;
  $("palette-split-hint").textContent = `${isMac ? "⌘" : "Ctrl+"}↵ open in split`;
  wrap().addEventListener("mousedown", (e) => {
    if (e.target === wrap()) closePalette();
  });
  input().addEventListener("input", () => {
    sel = 0;
    void update();
  });
  input().addEventListener("keydown", (e) => {
    if (e.key === "ArrowDown") {
      e.preventDefault();
      move(1);
    } else if (e.key === "ArrowUp") {
      e.preventDefault();
      move(-1);
    } else if (e.key === "Enter") {
      e.preventDefault();
      if (e.shiftKey) {
        const q = input().value.trim();
        if (q && !q.startsWith("#") && !q.startsWith(">")) {
          closePalette();
          app.create(q, newTab ? "tab" : "replace");
        }
        return;
      }
      pick(sel, e.ctrlKey || e.metaKey);
    } else if (e.key === "Escape") {
      e.preventDefault();
      e.stopPropagation();
      closePalette();
    } else if (e.key === "Tab") {
      e.preventDefault();
      move(e.shiftKey ? -1 : 1);
    }
  });
}

export function paletteOpen() {
  return !wrap().hidden;
}

export function openPalette(opts: { newTab?: boolean; query?: string } = {}) {
  newTab = !!opts.newTab;
  wrap().hidden = false;
  input().value = opts.query ?? "";
  input().placeholder = newTab ? "Open a note in a new tab…" : "Search notes, #tags, or run a command…";
  sel = 0;
  input().focus();
  void update();
}

export function closePalette() {
  wrap().hidden = true;
  seq++;
}

function move(step: number) {
  if (!items.length) return;
  sel = (sel + step + items.length) % items.length;
  render();
}

function pick(i: number, ctrl: boolean) {
  const it = items[i];
  if (!it) return;
  if (it.kind === "tag") {
    it.run("replace");
    return;
  }
  closePalette();
  it.run(ctrl ? "split" : newTab ? "tab" : "replace");
}

const noteItem = (path: string, extra: Partial<Item> = {}): Item => ({
  kind: "note",
  label: displayName(path),
  hint: parentOf(path) || "Vault",
  icon: isCanvasPath(path) ? "squares-four" : /^\d{4}-\d{2}-\d{2}$/.test(displayName(path)) ? "calendar-blank" : "file-text",
  path,
  run: (how) => app.open(path, how, extra.at),
  ...extra,
});

const cmdItem = (c: Command): Item => ({ kind: "cmd", label: c.label, hint: c.hint ?? "", icon: c.icon, run: () => c.run() });

async function update() {
  const mine = ++seq;
  const raw = input().value;
  const q = raw.trim();
  let next: Item[];
  if (!q) {
    const recent = app.recent().slice(0, 5).map((p) => noteItem(p));
    next = [...recent, ...app.commands().slice(0, newTab ? 0 : 5).map(cmdItem)];
  } else if (q.startsWith(">")) {
    const c = q.slice(1).trim().toLowerCase();
    next = app.commands().filter((x) => x.label.toLowerCase().includes(c)).map(cmdItem);
  } else if (q.startsWith("#")) {
    const t = q.slice(1).trim();
    const tags = await api.tags().catch(() => []);
    if (mine !== seq) return;
    const exact = tags.find((x) => x.tag.slice(1).toLowerCase() === t.toLowerCase());
    const tagItems: Item[] = tags
      .filter((x) => x.tag.slice(1).toLowerCase().startsWith(t.toLowerCase()) && x !== exact)
      .slice(0, 6)
      .map((x) => ({
        kind: "tag",
        label: x.tag,
        hint: `${x.count} ${x.count === 1 ? "note" : "notes"}`,
        icon: "hash",
        run: () => {
          input().value = x.tag + " ";
          sel = 0;
          void update();
        },
      }));
    let notes: Item[] = [];
    if (t) {
      const r = await api.search(`tag:#${t.split(/\s+/)[0]}`, 30).catch(() => null);
      if (mine !== seq) return;
      notes = (r?.hits ?? []).map((x) => noteItem(x.path));
    }
    next = [...tagItems, ...notes];
  } else {
    const [files, body] = await Promise.all([
      api.findFiles(q, app.source(), 20).catch(() => []),
      q.length >= 2 ? api.search(q, 12).catch(() => null) : Promise.resolve(null),
    ]);
    if (mine !== seq) return;
    const titleHits = files.filter((f) => f.isNote || isCanvasPath(f.path)).slice(0, 8);
    const have = new Set(titleHits.map((f) => f.path));
    const cmds = newTab ? [] : app.commands().filter((c) => c.label.toLowerCase().includes(q.toLowerCase())).slice(0, 4);
    const bodyHits: Item[] = [];
    for (const hit of body?.hits ?? []) {
      if (have.has(hit.path)) continue;
      const s = hit.snippets[0];
      if (!s) continue;
      bodyHits.push(noteItem(hit.path, { snip: { text: s.text, ranges: s.ranges }, at: { line: s.line, ch: s.ch, len: s.len } }));
      if (bodyHits.length >= 6) break;
    }
    next = [...titleHits.map((f) => noteItem(f.path)), ...cmds.map(cmdItem), ...bodyHits];
    const exists = titleHits.some((f) => displayName(f.path).toLowerCase() === q.toLowerCase());
    if (!exists) {
      next.push({
        kind: "new",
        label: `Create “${q}”`,
        hint: "New note",
        icon: "plus",
        run: (how) => app.create(q, how),
      });
    }
  }
  items = next;
  sel = Math.min(sel, Math.max(items.length - 1, 0));
  render();
}

function render() {
  const list = $("palette-list");
  if (items.length === 0) {
    list.replaceChildren(h("div.p-empty", null, "Nothing matches."));
    return;
  }
  list.replaceChildren(
    ...items.map((it, i) => {
      const lbl = h("span.lbl", null, h("span.l", it.kind === "note" ? { translate: "no" } : null, it.label));
      if (it.snip) {
        const s = h("span.s");
        appendHighlighted(s, it.snip.text, it.snip.ranges);
        lbl.append(s);
      }
      const b = h(
        `button.p-item.${it.kind}` as "button",
        { role: "option", "aria-selected": String(i === sel) },
        icon(it.icon),
        lbl,
        h("span.h", null, it.hint),
      );
      b.addEventListener("mousemove", () => {
        if (sel !== i) {
          sel = i;
          for (const [k, el] of [...list.children].entries()) el.setAttribute("aria-selected", String(k === i));
        }
      });
      b.addEventListener("mousedown", (e) => {
        e.preventDefault();
        pick(i, e.ctrlKey || e.metaKey);
      });
      return b;
    }),
  );
  list.children[sel]?.scrollIntoView({ block: "nearest" });
}
