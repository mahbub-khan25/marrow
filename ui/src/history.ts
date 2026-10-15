// Version history: earlier copies of a note (kept every few minutes while it
// changes), what changed since each one, and a way back.

import { api, displayName, type NoteVersion } from "./api";
import { h, icon } from "./util";

export interface HistoryOptions {
  path: string;
  /** The note as it is now. */
  current: () => string;
  /** Puts `text` back as the note. */
  restore: (text: string) => void;
  toast: (text: string, kind?: "info" | "error") => void;
}

// ------------------------------------------------------------------ diff

export type DiffLine = { kind: "same" | "add" | "del"; text: string };

/** Line diff from `a` to `b` (longest common subsequence). */
export function diffLines(a: string, b: string): DiffLine[] {
  const x = a.split("\n");
  const y = b.split("\n");
  // Trim the common start and end; most edits are small.
  let start = 0;
  while (start < x.length && start < y.length && x[start] === y[start]) start++;
  let endX = x.length;
  let endY = y.length;
  while (endX > start && endY > start && x[endX - 1] === y[endY - 1]) {
    endX--;
    endY--;
  }
  const out: DiffLine[] = x.slice(0, start).map((t) => ({ kind: "same", text: t }));
  const xs = x.slice(start, endX);
  const ys = y.slice(start, endY);
  const n = xs.length;
  const m = ys.length;
  if (n * m > 6_000_000) {
    // Too big to compare line by line: show it as replaced.
    out.push(...xs.map((t) => ({ kind: "del" as const, text: t })), ...ys.map((t) => ({ kind: "add" as const, text: t })));
  } else {
    // lcs[i][j] = LCS length of xs[i..] and ys[j..]
    const w = m + 1;
    const lcs = new Uint32Array((n + 1) * w);
    for (let i = n - 1; i >= 0; i--) {
      for (let j = m - 1; j >= 0; j--) {
        lcs[i * w + j] = xs[i] === ys[j] ? lcs[(i + 1) * w + j + 1] + 1 : Math.max(lcs[(i + 1) * w + j], lcs[i * w + j + 1]);
      }
    }
    let i = 0;
    let j = 0;
    while (i < n && j < m) {
      if (xs[i] === ys[j]) {
        out.push({ kind: "same", text: xs[i] });
        i++;
        j++;
      } else if (lcs[(i + 1) * w + j] >= lcs[i * w + j + 1]) {
        out.push({ kind: "del", text: xs[i++] });
      } else {
        out.push({ kind: "add", text: ys[j++] });
      }
    }
    while (i < n) out.push({ kind: "del", text: xs[i++] });
    while (j < m) out.push({ kind: "add", text: ys[j++] });
  }
  out.push(...x.slice(endX).map((t) => ({ kind: "same" as const, text: t })));
  return out;
}

// -------------------------------------------------------------------- UI

const dayLabel = (t: number) => {
  const d = new Date(t);
  const today = new Date();
  const yesterday = new Date(Date.now() - 86_400_000);
  if (d.toDateString() === today.toDateString()) return "Today";
  if (d.toDateString() === yesterday.toDateString()) return "Yesterday";
  return d.toLocaleDateString(undefined, { weekday: "short", day: "numeric", month: "short", year: d.getFullYear() === today.getFullYear() ? undefined : "numeric" });
};
const timeLabel = (t: number) => new Date(t).toLocaleTimeString(undefined, { hour: "2-digit", minute: "2-digit" });

function ago(t: number): string {
  const s = Math.round((Date.now() - t) / 1000);
  if (s < 60) return "just now";
  const m = Math.round(s / 60);
  if (m < 60) return `${m} min ago`;
  const hr = Math.round(m / 60);
  if (hr < 24) return `${hr} h ago`;
  const d = Math.round(hr / 24);
  return `${d} ${d === 1 ? "day" : "days"} ago`;
}

export async function openHistory(o: HistoryOptions) {
  const versions: NoteVersion[] = await api.historyList(o.path).catch(() => []);
  const name = displayName(o.path);
  let picked: NoteVersion | null = versions[0] ?? null;
  let view: "changes" | "text" = "changes";
  const cache = new Map<number, string>();

  const list = h("div.hist-list", { role: "listbox", "aria-label": "Versions" });
  const body = h("div.hist-body");
  const legend = h("div.hist-legend");
  const restoreBtn = h("button.btn.btn-primary", { onclick: () => void restore() }, icon("arrow-counter-clockwise"), "Restore this version");
  const copyBtn = h("button.btn.btn-secondary", { onclick: () => void copy() }, icon("copy"), "Copy text");
  const segChanges = h("button", { "aria-pressed": "true", onclick: () => setView("changes") }, "Changes");
  const segText = h("button", { "aria-pressed": "false", onclick: () => setView("text") }, "Full text");

  const card = h(
    "div.hist",
    { role: "dialog", "aria-label": `Version history of ${name}` },
    h(
      "div.hist-head",
      null,
      h("div.hist-title", null, icon("clock-counter-clockwise"), h("span", null, "Version history"), h("span.hist-name", null, name)),
      h("button.icon-btn", { title: "Close (Esc)", "aria-label": "Close", onclick: () => close() }, icon("x")),
    ),
    versions.length
      ? h(
          "div.hist-main",
          null,
          list,
          h(
            "div.hist-view",
            null,
            h("div.hist-tools", null, h("div.seg.small", { role: "group" }, segChanges, segText), legend),
            body,
            h("div.hist-foot", null, copyBtn, restoreBtn),
          ),
        )
      : h(
          "div.hist-empty",
          null,
          icon("clock-counter-clockwise"),
          h("p", null, "No earlier versions of this note yet."),
          h("p.d", null, "While you edit, Marrow keeps a copy of the note every few minutes, so you can go back later."),
        ),
  );
  const wrap = h("div.backdrop.hist-wrap", null, card);
  wrap.addEventListener("mousedown", (e) => {
    if (e.target === wrap) close();
  });
  document.body.append(wrap);
  document.documentElement.toggleAttribute("data-modal", true);
  const keys = (e: KeyboardEvent) => {
    if (e.key === "Escape") {
      e.preventDefault();
      e.stopPropagation();
      close();
    } else if ((e.key === "ArrowDown" || e.key === "ArrowUp") && versions.length) {
      e.preventDefault();
      const i = picked ? versions.indexOf(picked) : -1;
      const next = versions[Math.max(0, Math.min(versions.length - 1, i + (e.key === "ArrowDown" ? 1 : -1)))];
      if (next) void pick(next);
    }
  };
  document.addEventListener("keydown", keys, true);

  function close() {
    wrap.remove();
    document.removeEventListener("keydown", keys, true);
    document.documentElement.toggleAttribute("data-modal", false);
  }

  function renderList() {
    list.replaceChildren();
    let day = "";
    for (const v of versions) {
      const d = dayLabel(v.timeMs);
      if (d !== day) {
        day = d;
        list.append(h("div.hist-day", null, d));
      }
      list.append(
        h(
          "button.hist-item",
          { role: "option", "aria-selected": String(v === picked), onclick: () => void pick(v) },
          h("span.t", null, timeLabel(v.timeMs)),
          h("span.a", null, ago(v.timeMs)),
        ),
      );
    }
  }

  async function text(v: NoteVersion): Promise<string> {
    if (!cache.has(v.timeMs)) cache.set(v.timeMs, await api.historyRead(o.path, v.timeMs));
    return cache.get(v.timeMs)!;
  }

  function setView(v: typeof view) {
    view = v;
    segChanges.setAttribute("aria-pressed", String(v === "changes"));
    segText.setAttribute("aria-pressed", String(v === "text"));
    void show();
  }

  async function pick(v: NoteVersion) {
    picked = v;
    renderList();
    list.querySelector<HTMLElement>('[aria-selected="true"]')?.scrollIntoView({ block: "nearest" });
    await show();
  }

  async function show() {
    if (!picked) return;
    const v = picked;
    let old: string;
    try {
      old = await text(v);
    } catch (e) {
      body.replaceChildren(h("p.hist-error", null, `Couldn't read this version: ${e}`));
      return;
    }
    if (picked !== v) return;
    const now = o.current();
    if (view === "text") {
      legend.textContent = `As it was ${dayLabel(v.timeMs).toLowerCase()} at ${timeLabel(v.timeMs)}`;
      body.replaceChildren(h("pre.hist-text", null, old));
      return;
    }
    const d = diffLines(old, now);
    const added = d.filter((l) => l.kind === "add").length;
    const removed = d.filter((l) => l.kind === "del").length;
    legend.replaceChildren(
      added || removed
        ? h("span", null, "Since then: ", h("span.add", null, `${added} ${added === 1 ? "line" : "lines"} added`), ", ", h("span.del", null, `${removed} removed`))
        : h("span", null, "Same as the note now."),
    );
    restoreBtn.toggleAttribute("disabled", old === now);
    const out = h("div.hist-diff");
    // Long unchanged stretches fold away, keeping 2 lines of context.
    for (let i = 0; i < d.length; ) {
      if (d[i].kind === "same") {
        let j = i;
        while (j < d.length && d[j].kind === "same") j++;
        const run = d.slice(i, j);
        const first = i === 0;
        const last = j === d.length;
        const keepTop = first ? 0 : 2;
        const keepBottom = last ? 0 : 2;
        if (run.length > keepTop + keepBottom + 3) {
          for (const l of run.slice(0, keepTop)) out.append(line(l));
          out.append(h("div.hist-fold", null, `${run.length - keepTop - keepBottom} unchanged ${run.length - keepTop - keepBottom === 1 ? "line" : "lines"}`));
          for (const l of run.slice(run.length - keepBottom)) out.append(line(l));
        } else for (const l of run) out.append(line(l));
        i = j;
      } else out.append(line(d[i++]));
    }
    body.replaceChildren(out);
  }

  function line(l: DiffLine): HTMLElement {
    return h(`div.hist-line.${l.kind}` as "div", null, h("span.m", null, l.kind === "add" ? "+" : l.kind === "del" ? "−" : ""), h("span.x", null, l.text || " "));
  }

  async function restore() {
    if (!picked) return;
    const old = await text(picked).catch(() => null);
    if (old === null) return;
    // The note as it is now becomes a version too, so the restore can be undone.
    await api.historySnapshot(o.path, o.current()).catch(() => {});
    o.restore(old);
    o.toast(`Restored the version from ${dayLabel(picked.timeMs).toLowerCase()} at ${timeLabel(picked.timeMs)}. The text before it is in the history too.`);
    close();
  }

  async function copy() {
    if (!picked) return;
    const old = await text(picked).catch(() => null);
    if (old === null) return;
    await navigator.clipboard.writeText(old);
    o.toast("Copied that version's text.");
  }

  renderList();
  if (picked) await show();
}
