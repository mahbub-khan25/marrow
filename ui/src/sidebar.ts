// The Search and Tags views of the sidebar. (Files lives in tree.ts.)

import { ask } from "@tauri-apps/plugin-dialog";

import { aiConfig } from "./ai";
import { api, displayName, parentOf, type Recall, type SearchResults, type TagCount } from "./api";
import { editNote } from "./database";
import { $, h } from "./util";

export interface SidebarApp {
  /** Opens a note, optionally at a place, in a new tab if `newTab`. */
  openNote: (path: string, at?: { line: number; ch: number; len?: number }, newTab?: boolean) => void;
  toast?: (text: string, kind?: "info" | "error") => void;
}

/** The text a search looks for, without tag:/path:/file: and -word parts. */
export function searchPhrase(q: string): string {
  const parts: string[] = [];
  for (const m of q.matchAll(/(-?)("([^"]*)"|\S+)/g)) {
    if (m[1]) continue;
    const tok = m[3] ?? m[2];
    if (m[3] === undefined && /^(tag|path|file):/i.test(tok)) continue;
    if (m[3] === undefined && /^#\S/.test(tok)) continue;
    parts.push(tok);
  }
  return parts.join(" ").trim();
}

const escapeRe = (s: string) => s.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");

export class SearchView {
  private seq = 0;
  private timer: number | undefined;
  private input = $<HTMLInputElement>("search-input");

  constructor(private app: SidebarApp) {
    $("replace-all").addEventListener("click", () => void this.replaceAll());
    $<HTMLInputElement>("replace-input").addEventListener("keydown", (e) => {
      if (e.key === "Enter") void this.replaceAll();
      else if (e.key === "Escape") {
        e.stopPropagation();
        this.showReplace(false);
        this.input.focus();
      }
    });
    this.input.addEventListener("input", () => {
      window.clearTimeout(this.timer);
      this.timer = window.setTimeout(() => void this.run(), 140);
    });
    this.input.addEventListener("keydown", (e) => {
      if (e.key === "Enter") {
        $("search-results").querySelector<HTMLElement>(".sr-head")?.click();
      } else if (e.key === "Escape" && this.input.value) {
        e.stopPropagation();
        this.input.value = "";
        void this.run();
      }
    });
  }

  focus(query?: string) {
    if (query !== undefined) {
      this.input.value = query;
      void this.run();
    }
    this.input.focus();
    this.input.select();
  }

  /** Shows or hides the Replace box under the search box. */
  showReplace(on = $("replace-row").hidden) {
    $("replace-row").hidden = !on;
    if (on) $<HTMLInputElement>("replace-input").focus();
    void this.run();
  }

  /** Replaces the searched phrase in every matching note. */
  private async replaceAll() {
    const q = this.input.value.trim();
    const phrase = searchPhrase(q);
    const toast = this.app.toast ?? (() => {});
    if (!phrase) return toast("Search for the text to replace first.", "error");
    const replacement = $<HTMLInputElement>("replace-input").value;
    const r = await api.search(q, 100000).catch(() => null);
    if (!r || !r.hits.length) return toast("Nothing matches.");
    const re = new RegExp(escapeRe(phrase), "gi");
    let count = 0;
    const counts = new Map<string, number>();
    for (const hit of r.hits) {
      const text = await api.readNote(hit.path).catch(() => "");
      const n = (text.match(re) ?? []).length;
      if (n) counts.set(hit.path, n);
      count += n;
    }
    if (!count) return toast(`“${phrase}” only matches note names, not text in them.`);
    const ok = await ask(
      `Replace ${count} ${count === 1 ? "match" : "matches"} of “${phrase}” with “${replacement}” in ${counts.size} ${counts.size === 1 ? "note" : "notes"}?`,
      { title: "Replace all", kind: "warning", okLabel: "Replace all", cancelLabel: "Cancel" },
    );
    if (!ok) return;
    let done = 0;
    for (const path of counts.keys()) {
      try {
        await editNote(path, (text) => text.replace(re, () => replacement));
        done++;
      } catch (e) {
        toast(`Couldn't change “${displayName(path)}”: ${e}`, "error");
      }
    }
    toast(`Replaced ${count} ${count === 1 ? "match" : "matches"} in ${done} ${done === 1 ? "note" : "notes"}.`);
    window.setTimeout(() => void this.run(), 400);
  }

  reset() {
    this.input.value = "";
    void this.run();
  }

  refresh() {
    if (this.input.value.trim()) void this.run();
  }

  /** Words (exact) or meaning (embeddings, when set up in Settings → AI). */
  private mode: "words" | "meaning" = "words";

  private modeSwitch(): HTMLElement | null {
    if (!aiConfig().embed) return null;
    const b = (m: "words" | "meaning", label: string) =>
      h(
        "button",
        {
          type: "button",
          "aria-pressed": String(this.mode === m),
          onclick: () => {
            this.mode = m;
            void this.run();
          },
        },
        label,
      );
    return h("div.seg.tiny.search-mode", { role: "group", "aria-label": "Search by" }, b("words", "Words"), b("meaning", "Meaning"));
  }

  private async runMeaning(q: string, mine: number) {
    const list = $("search-results");
    const summary = $("search-summary");
    summary.replaceChildren("Looking…");
    let r: Recall;
    try {
      r = await api.aiRecall(q, 20, "meaning");
    } catch (e) {
      if (mine === this.seq) summary.replaceChildren(String(e));
      return;
    }
    if (mine !== this.seq) return;
    const sw = this.modeSwitch();
    summary.replaceChildren(...(sw ? [sw] : []), r.hits.length ? ` Closest in meaning` : " Nothing close in meaning.");
    if (r.note) summary.append(h("div", null, r.note));
    // One entry per note, its best parts under it.
    const byNote = new Map<string, Recall["hits"]>();
    for (const hit of r.hits) (byNote.get(hit.path) ?? byNote.set(hit.path, []).get(hit.path)!).push(hit);
    list.replaceChildren(
      ...[...byNote.values()].map((hits) => {
        const first = hits[0];
        const box = h(
          "div.sr",
          null,
          h(
            "button.sr-head",
            { title: first.path, onclick: (e: MouseEvent) => this.app.openNote(first.path, { line: first.line, ch: 0 }, e.ctrlKey || e.metaKey) },
            h("span.sr-title", null, first.name),
            h("span.sr-folder", null, parentOf(first.path)),
          ),
        );
        for (const hit of hits.slice(0, 2)) {
          const text = hit.text
            .replace(/!?\[\[([^\]|]+)(?:\|([^\]]+))?\]\]/g, (_m, a: string, b?: string) => b ?? a)
            .replace(/[*_`#>]+/g, "")
            .replace(/\s+/g, " ")
            .trim();
          box.append(
            h(
              "button.sr-snip",
              { onclick: (e: MouseEvent) => this.app.openNote(hit.path, { line: hit.line, ch: 0 }, e.ctrlKey || e.metaKey) },
              hit.heading ? h("b", null, `${hit.heading} · `) : null,
              text.length > 180 ? text.slice(0, 180) + "…" : text,
            ),
          );
        }
        return box;
      }),
    );
  }

  async run() {
    const q = this.input.value.trim();
    const mine = ++this.seq;
    const list = $("search-results");
    const summary = $("search-summary");
    if (q && this.mode === "meaning" && aiConfig().embed) return this.runMeaning(q, mine);
    if (!q) {
      list.replaceChildren();
      summary.replaceChildren(
        "Searches titles and full text across the vault. Every word must appear. Try ",
        h("code", null, '"exact phrase"'),
        ", ",
        h("code", null, "-word"),
        ", ",
        h("code", null, "tag:#idea"),
        ", ",
        h("code", null, "path:Daily"),
        " or ",
        h("code", null, "file:plan"),
        ".",
      );
      return;
    }
    let r: SearchResults;
    try {
      r = await api.search(q, 100);
    } catch (e) {
      if (mine === this.seq) summary.textContent = String(e);
      return;
    }
    if (mine !== this.seq) return;
    const matches = r.hits.reduce((s, x) => s + x.matches, 0);
    const replaceOn = !$("replace-row").hidden;
    summary.textContent =
      r.total === 0
        ? "No notes match."
        : `${matches ? `${matches} ${matches === 1 ? "match" : "matches"} in ` : ""}${r.total} ${r.total === 1 ? "note" : "notes"}${
            r.total > r.hits.length ? `, showing the first ${r.hits.length}` : ""
          }`;
    const sw = this.modeSwitch();
    if (sw) summary.prepend(sw, " ");
    if (r.total > 0 && searchPhrase(q)) {
      summary.append(" · ", h("button.link-btn", { type: "button", onclick: () => this.showReplace(!replaceOn) }, replaceOn ? "Hide replace" : "Replace…"));
    }
    list.replaceChildren(
      ...r.hits.map((hit) => {
        const first = hit.snippets[0];
        const head = h(
          "button.sr-head",
          {
            title: hit.path,
            onclick: (e: MouseEvent) =>
              this.app.openNote(hit.path, first ? { line: first.line, ch: first.ch, len: first.len } : undefined, e.ctrlKey || e.metaKey),
          },
          h("span.sr-title", null, hit.name),
          h("span.sr-folder", null, parentOf(hit.path)),
        );
        const box = h("div.sr", null, head);
        for (const s of hit.snippets.slice(0, 3)) {
          const b = h("button.sr-snip", {
            onclick: (e: MouseEvent) => this.app.openNote(hit.path, { line: s.line, ch: s.ch, len: s.len }, e.ctrlKey || e.metaKey),
          });
          appendHighlighted(b, s.text, s.ranges);
          box.append(b);
        }
        return box;
      }),
    );
  }
}

export class TagsView {
  selected: string | null = null;
  private seq = 0;

  constructor(private app: SidebarApp) {}

  select(tag: string | null) {
    this.selected = tag ? tag.replace(/^#/, "") : null;
    void this.refresh();
  }

  async refresh() {
    const mine = ++this.seq;
    const tags = await api.tags().catch(() => [] as TagCount[]);
    if (mine !== this.seq) return;
    const chips = $("tag-chips");
    if (tags.length === 0) {
      chips.replaceChildren(h("p.insp-empty", null, "No tags yet. Write #something in a note to add one."));
      $("tag-heading").textContent = "";
      $("tag-list").replaceChildren();
      return;
    }
    const sel = this.selected?.toLowerCase() ?? null;
    const sorted = [...tags].sort((a, b) => b.count - a.count || a.tag.localeCompare(b.tag));
    chips.replaceChildren(
      ...sorted.map((t) => {
        const name = t.tag.replace(/^#/, "");
        return h(
          "button.tag-chip",
          { "aria-pressed": String(name.toLowerCase() === sel), onclick: () => this.select(name) },
          `#${name}`,
          h("span", null, String(t.count)),
        );
      }),
    );
    $("tag-heading").textContent = this.selected ? `Tagged #${this.selected}` : "Pick a tag";
    const list = $("tag-list");
    if (!this.selected) {
      list.replaceChildren();
      return;
    }
    const r = await api.search(`tag:#${this.selected}`, 200).catch(() => null);
    if (mine !== this.seq || !r) return;
    list.replaceChildren(
      ...r.hits.map((hit) =>
        h(
          "button.note-row",
          { onclick: (e: MouseEvent) => this.app.openNote(hit.path, undefined, e.ctrlKey || e.metaKey) },
          h("span.t", { translate: "no" }, displayName(hit.path)),
          h("span.f", null, parentOf(hit.path) || "Vault"),
        ),
      ),
    );
  }
}

export function appendHighlighted(el: HTMLElement, text: string, ranges: [number, number][]) {
  let at = 0;
  for (const [a, b] of ranges) {
    if (a > at) el.append(text.slice(at, a));
    el.append(h("mark", null, text.slice(a, b)));
    at = b;
  }
  if (at < text.length) el.append(text.slice(at));
}
