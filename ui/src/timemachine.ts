// Time machine: drag through time and see a note, or the whole vault, as it
// was on any day. Built on the versions Marrow keeps while you write (see
// history.ts), so it reaches back as far as those go.

import { api, displayName, type ThenNote } from "./api";
import { diffLines } from "./history";
import { uiLocale } from "./i18n";
import { renderMarkdown } from "./preview";
import { h, icon } from "./util";

export interface TimeMachineOptions {
  /** The note to start with; without one, the whole vault. */
  path: string | null;
  /** A note's text now (unsaved edits included). */
  current(path: string): Promise<string>;
  /** Puts `text` back as the note. */
  restore(path: string, text: string): Promise<void>;
  /** Creates a deleted note again. */
  bringBack(path: string, text: string): Promise<void>;
  open(path: string): void;
  toast(text: string, kind?: "info" | "error"): void;
}

const DAY = 86_400_000;

const longDate = (t: number) =>
  new Date(t).toLocaleDateString(uiLocale(), { weekday: "long", day: "numeric", month: "long", year: "numeric" });
const clock = (t: number) => new Date(t).toLocaleTimeString(uiLocale(), { hour: "2-digit", minute: "2-digit" });

/** "3 months ago", "just now". */
export function agoLong(t: number, now = Date.now()): string {
  const d = (now - t) / DAY;
  if (d < 1 / 24) return "just now";
  if (d < 1) return `${Math.round(d * 24)} ${Math.round(d * 24) === 1 ? "hour" : "hours"} ago`;
  if (d < 2) return "yesterday";
  if (d < 14) return `${Math.round(d)} days ago`;
  if (d < 60) return `${Math.round(d / 7)} weeks ago`;
  if (d < 365) return `${Math.round(d / 30.4)} months ago`;
  const y = d / 365.25;
  return y < 1.5 ? "a year ago" : `${Math.round(y)} years ago`;
}

/** Splits [min, max] into `n` buckets and sums `days` (day index, count) into them. */
export function buckets(days: [number, number][], min: number, max: number, n: number): number[] {
  const out = new Array<number>(n).fill(0);
  const span = Math.max(1, max - min);
  for (const [day, count] of days) {
    const t = day * DAY + DAY / 2;
    if (t < min || t > max) continue;
    out[Math.min(n - 1, Math.floor(((t - min) / span) * n))] += count;
  }
  return out;
}

type Filter = "all" | "changed" | "gone" | "added";

export async function openTimeMachine(o: TimeMachineOptions) {
  const now = Date.now();
  const [timeline, nowList] = await Promise.all([
    api.historyTimeline().catch(() => ({ days: [] as [number, number][], firstMs: null })),
    api.historyVaultAt(now + 60_000).catch(() => [] as ThenNote[]),
  ]);
  const nowSet = new Set(nowList.map((n) => n.path));
  const firstDay = timeline.days.length ? timeline.days[0][0] * DAY : now;
  const minT = Math.min(firstDay, timeline.firstMs ?? now, now - DAY);
  const maxT = now;

  let mode: "note" | "vault" = o.path ? "note" : "vault";
  let notePath = o.path;
  let selected: string | null = o.path;
  let t = Math.max(minT, now - 30 * DAY);
  let view: "then" | "changes" = "then";
  let filter: Filter = "all";
  let thens: ThenNote[] = [];
  let noteVersions: number[] = [];
  const texts = new Map<string, string>();

  // ---- head
  const segNote = h("button", { onclick: () => setMode("note") }, "This note");
  const segVault = h("button", { onclick: () => setMode("vault") }, "Whole vault");
  const head = h(
    "div.hist-head",
    null,
    h("div.hist-title", null, icon("clock-counter-clockwise"), h("span", null, "Time machine")),
    h("div.seg.small", { role: "group", "aria-label": "Show" }, segNote, segVault),
    h("button.icon-btn", { title: "Close (Esc)", "aria-label": "Close", onclick: () => close() }, icon("x")),
  );

  // ---- when
  const dateEl = h("div.tm-date");
  const agoEl = h("div.tm-ago");
  const bars = h("div.tm-bars", { "aria-hidden": "true" });
  const range = h("input.tm-range", { type: "range", min: "0", max: "1000", step: "1", "aria-label": "Date" }) as HTMLInputElement;
  const earlier = h("button.btn.btn-secondary.tm-step", { title: "The change before this one", onclick: () => step(-1) }, icon("caret-left"), "Earlier change");
  const later = h("button.btn.btn-secondary.tm-step", { title: "The change after this one", onclick: () => step(1) }, "Later change", icon("caret-right"));
  const jumps = h("div.tm-jumps");
  const when = h(
    "div.tm-when",
    null,
    h("div.tm-label", null, dateEl, agoEl),
    h("div.tm-track", null, bars, range),
    h("div.tm-under", null, earlier, jumps, later),
  );

  // ---- main
  const summary = h("div.tm-sum");
  const list = h("div.tm-list", { role: "listbox", "aria-label": "Notes then", translate: "no" });
  const status = h("div.tm-status");
  const segThen = h("button", { onclick: () => setView("then") }, "Then");
  const segChanges = h("button", { onclick: () => setView("changes") }, "Changes since");
  const page = h("div.tm-page");
  const copyBtn = h("button.btn.btn-secondary", { onclick: () => void copy() }, icon("copy"), "Copy text");
  const openBtn = h("button.btn.btn-secondary", { onclick: () => selected && nowSet.has(selected) && (close(), o.open(selected)) }, icon("arrow-square-out"), "Open the note");
  const restoreBtn = h("button.btn.btn-primary", { onclick: () => void restore() }, icon("arrow-counter-clockwise"), "Restore this version");
  const viewer = h(
    "div.tm-viewer",
    null,
    h("div.hist-tools", null, h("div.seg.small", { role: "group" }, segThen, segChanges), status),
    page,
    h("div.hist-foot", null, openBtn, copyBtn, restoreBtn),
  );
  const main = h("div.tm-main", null, h("div.tm-side", null, summary, list), viewer);

  const card = h("div.hist.tm", { role: "dialog", "aria-label": "Time machine" }, head, when, main);
  const wrap = h("div.backdrop.hist-wrap", null, card);
  wrap.addEventListener("mousedown", (e) => e.target === wrap && close());
  document.body.append(wrap);
  document.documentElement.toggleAttribute("data-modal", true);
  const keys = (e: KeyboardEvent) => {
    if (e.key === "Escape") {
      e.preventDefault();
      e.stopPropagation();
      close();
    }
  };
  document.addEventListener("keydown", keys, true);
  function close() {
    wrap.remove();
    document.removeEventListener("keydown", keys, true);
    document.documentElement.toggleAttribute("data-modal", false);
  }

  // ---- time <-> slider
  const toT = (v: number) => Math.round(minT + ((maxT - minT) * v) / 1000);
  const toV = (x: number) => Math.round(((x - minT) / Math.max(1, maxT - minT)) * 1000);
  let timer = 0;
  range.addEventListener("input", () => {
    t = toT(Number(range.value));
    renderWhen();
    window.clearTimeout(timer);
    timer = window.setTimeout(() => void refresh(), 120);
  });

  function jumpTo(x: number) {
    t = Math.max(minT, Math.min(maxT, x));
    range.value = String(toV(t));
    renderWhen();
    void refresh();
  }

  function renderJumps() {
    const options: [string, number][] = [
      ["A week ago", now - 7 * DAY],
      ["A month ago", now - 30 * DAY],
      ["6 months ago", now - 182 * DAY],
      ["A year ago", now - 365 * DAY],
      ["Now", now],
    ];
    jumps.replaceChildren(...options.filter(([, x]) => x >= minT).map(([label, x]) => h("button.chip", { onclick: () => jumpTo(x) }, label)));
  }

  function renderWhen() {
    dateEl.textContent = t >= now - 60_000 ? "Now" : longDate(t);
    agoEl.textContent = t >= now - 60_000 ? "Drag back to see how things were" : `${clock(t)} · ${agoLong(t, now)}`;
    // Step buttons: only in a note, where changes are known.
    const showSteps = mode === "note" && noteVersions.length > 0;
    earlier.hidden = later.hidden = !showSteps;
    earlier.toggleAttribute("disabled", !noteVersions.some((v) => v < t));
    later.toggleAttribute("disabled", t >= now - 60_000);
  }

  function renderBars() {
    const n = 90;
    let counts: number[];
    if (mode === "note") {
      counts = new Array<number>(n).fill(0);
      for (const v of noteVersions) counts[Math.min(n - 1, Math.floor(((v - minT) / Math.max(1, maxT - minT)) * n))] += 1;
    } else counts = buckets(timeline.days, minT, maxT, n);
    const top = Math.max(1, ...counts);
    bars.replaceChildren(...counts.map((c) => h("span", { style: `height:${c ? 12 + Math.sqrt(c / top) * 88 : 0}%` })));
  }

  function step(dir: -1 | 1) {
    const sorted = [...noteVersions].sort((a, b) => a - b);
    if (dir < 0) {
      // Just before the save that came before the version showing now.
      const showing = sorted.find((v) => v > t);
      const before = sorted.filter((v) => v < (showing ?? Infinity) && v <= t);
      const target = before.length ? before[before.length - 1] - 1 : null;
      if (target !== null) jumpTo(target);
    } else {
      const next = sorted.find((v) => v > t);
      jumpTo(next !== undefined ? next : now);
    }
  }

  // ---- data
  async function refresh() {
    const at = t;
    try {
      thens = await api.historyVaultAt(at);
    } catch (e) {
      status.textContent = String(e);
      return;
    }
    if (at !== t) return;
    renderSummary();
    if (mode === "vault") renderList();
    void show();
  }

  function entries(): { path: string; state: ThenNote["state"] | "added" }[] {
    if (filter === "added") {
      const then = new Set(thens.map((x) => x.path));
      return nowList.filter((n) => !then.has(n.path)).map((n) => ({ path: n.path, state: "added" as const }));
    }
    return thens.filter((x) => filter === "all" || x.state === filter);
  }

  function renderSummary() {
    if (mode === "note") {
      summary.replaceChildren();
      return;
    }
    const changed = thens.filter((x) => x.state === "changed").length;
    const gone = thens.filter((x) => x.state === "gone").length;
    const thenSet = new Set(thens.map((x) => x.path));
    const added = nowList.filter((n) => !thenSet.has(n.path)).length;
    const chip = (f: Filter, label: string, n: number) =>
      h("button.chip", { "aria-pressed": String(filter === f), onclick: () => ((filter = f), renderSummary(), renderList()) }, `${label} `, h("b", null, n.toLocaleString()));
    summary.replaceChildren(
      chip("all", "Notes then", thens.length),
      chip("changed", "Changed since", changed),
      chip("added", "Added since", added),
      chip("gone", "Deleted since", gone),
    );
  }

  function renderList() {
    const items = entries();
    if (!items.length) {
      list.replaceChildren(h("p.tm-none", null, filter === "all" ? "No notes from that far back." : "None."));
      return;
    }
    const shown = items.slice(0, 400);
    list.replaceChildren(
      ...shown.map((x) => {
        const folder = x.path.includes("/") ? x.path.slice(0, x.path.lastIndexOf("/")) : "";
        return h(
          "button.tm-item",
          { role: "option", "aria-selected": String(x.path === selected), "data-state": x.state, onclick: () => ((selected = x.path), renderList(), void show()) },
          h("span.n", null, displayName(x.path)),
          folder ? h("span.f", null, folder) : null,
          x.state === "changed" ? h("span.s", { title: "Changed since" }, "changed") : x.state === "gone" ? h("span.s", { title: "Deleted since" }, "deleted") : x.state === "added" ? h("span.s", null, "new") : null,
        );
      }),
      items.length > shown.length ? h("p.tm-none", null, `…and ${items.length - shown.length} more. Use the filters above.`) : h("span"),
    );
  }

  async function textOf(path: string, version: number | null): Promise<string> {
    const key = `${path}@${version ?? "now"}`;
    if (version === null) return o.current(path);
    if (!texts.has(key)) texts.set(key, await api.historyRead(path, version));
    return texts.get(key)!;
  }

  let thenText: string | null = null;
  async function show() {
    const path = mode === "note" ? notePath : selected;
    thenText = null;
    restoreBtn.hidden = copyBtn.hidden = openBtn.hidden = true;
    if (!path) {
      status.textContent = "";
      page.replaceChildren(h("div.hist-empty", null, icon("clock-counter-clockwise"), h("p", null, "Pick a note on the left to see it as it was.")));
      return;
    }
    const entry = thens.find((x) => x.path === path);
    if (!entry) {
      status.textContent = "";
      page.replaceChildren(
        h(
          "div.hist-empty",
          null,
          icon("plant"),
          h("p", null, nowSet.has(path) ? "This note didn't exist yet." : "Nothing known about this note then."),
          h("p.d", null, nowSet.has(path) ? "Drag the slider later to see it appear." : ""),
        ),
      );
      openBtn.hidden = !nowSet.has(path);
      return;
    }
    const at = t;
    let then: string;
    let nowText = "";
    try {
      then = await textOf(path, entry.state === "same" ? null : entry.version);
      if (entry.state !== "gone") nowText = await o.current(path);
    } catch (e) {
      page.replaceChildren(h("p.hist-error", null, `Couldn't read it: ${e}`));
      return;
    }
    if (at !== t) return;
    thenText = then;
    const approximate = entry.state === "changed" && entry.version === null;
    status.textContent =
      entry.state === "same"
        ? "Same as it is now."
        : entry.state === "gone"
          ? "Deleted since. You can bring it back."
          : approximate
            ? "Changed since, but no copy from then was kept: this is how it is now."
            : "Changed since.";
    copyBtn.hidden = false;
    openBtn.hidden = !nowSet.has(path);
    restoreBtn.hidden = entry.state === "same" || approximate;
    restoreBtn.replaceChildren(icon(entry.state === "gone" ? "arrow-u-up-left" : "arrow-counter-clockwise"), entry.state === "gone" ? "Bring it back" : "Restore this version");
    segChanges.toggleAttribute("disabled", entry.state === "same");
    if (view === "changes" && entry.state !== "same") {
      const d = diffLines(then, nowText);
      page.replaceChildren(
        h(
          "div.hist-diff",
          null,
          ...d.map((l) => h(`div.hist-line.${l.kind}` as "div", null, h("span.m", null, l.kind === "add" ? "+" : l.kind === "del" ? "−" : ""), h("span.x", null, l.text || " "))),
        ),
      );
    } else {
      const el = h("div.preview.tm-preview", { translate: "no" });
      el.innerHTML = renderMarkdown(then, new Set());
      // The note's own first heading usually is its title already.
      const body = then.replace(/^---\n[\s\S]*?\n---\n/, "").trimStart();
      page.replaceChildren(...(/^#\s/.test(body) ? [] : [h("h1.tm-title", { translate: "no" }, displayName(path))]), el);
    }
  }

  function setView(v: typeof view) {
    view = v;
    segThen.setAttribute("aria-pressed", String(v === "then"));
    segChanges.setAttribute("aria-pressed", String(v === "changes"));
    void show();
  }

  async function setMode(m: typeof mode) {
    if (m === "note" && !notePath) {
      notePath = selected;
      if (!notePath) return o.toast("Pick a note first.");
    }
    mode = m;
    segNote.setAttribute("aria-pressed", String(m === "note"));
    segVault.setAttribute("aria-pressed", String(m === "vault"));
    segNote.toggleAttribute("disabled", !notePath && !selected);
    card.dataset.mode = m;
    noteVersions = m === "note" && notePath ? (await api.historyList(notePath).catch(() => [])).map((v) => v.timeMs) : [];
    if (m === "note" && notePath) {
      head.querySelector(".hist-title")!.replaceChildren(icon("clock-counter-clockwise"), h("span", null, "Time machine"), h("span.hist-name", { translate: "no" }, displayName(notePath)));
      selected = notePath;
    } else head.querySelector(".hist-title")!.replaceChildren(icon("clock-counter-clockwise"), h("span", null, "Time machine"));
    renderBars();
    renderWhen();
    await refresh();
  }

  async function restore() {
    const path = mode === "note" ? notePath : selected;
    const entry = thens.find((x) => x.path === path);
    if (!path || !entry || thenText === null) return;
    try {
      if (entry.state === "gone") {
        await o.bringBack(path, thenText);
        o.toast(`Brought back “${displayName(path)}”.`);
      } else {
        // The note as it is now is kept first, so the restore can be undone.
        await api.historySnapshot(path, await o.current(path)).catch(() => {});
        await o.restore(path, thenText);
        o.toast(`Restored “${displayName(path)}” as it was ${agoLong(t)}. The text before it is in its history too.`);
      }
      close();
    } catch (e) {
      o.toast(String(e), "error");
    }
  }

  async function copy() {
    if (thenText === null) return;
    await navigator.clipboard.writeText(thenText);
    o.toast("Copied.");
  }

  range.value = String(toV(t));
  renderJumps();
  setView("then");
  await setMode(mode);
  range.focus();
}
