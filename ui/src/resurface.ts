// Resurfacing: notes come back to you. On the start screen, a note you
// haven't opened in a long time and the same day in earlier years; once a
// week, a gentle review of what happened, what's open and what's worth
// remembering, saved as a note if you like.

import { api, displayName, type NoteRow, type TaskItem } from "./api";
import { onThisDay } from "./daily";
import { uiLocale } from "./i18n";
import { h, icon, isoDay, plainText, storage, store } from "./util";

const DAY = 86_400_000;

// ------------------------------------------------------------------ seen

const seenKey = (vault: string) => `marrow.seen.${vault}`;

/** Remembers that `path` was opened now. */
export function markSeen(vault: string, path: string, now = Date.now()) {
  const seen = storage<Record<string, number>>(seenKey(vault), {});
  seen[path] = now;
  store(seenKey(vault), seen);
}

export function seenTimes(vault: string): Record<string, number> {
  return storage<Record<string, number>>(seenKey(vault), {});
}

/** Follows a rename so the note keeps its "last opened". */
export function seenMoved(vault: string, from: string, to: string) {
  const seen = seenTimes(vault);
  for (const k of Object.keys(seen)) {
    if (k === from || k.startsWith(from + "/")) {
      seen[to + k.slice(from.length)] = seen[k];
      delete seen[k];
    }
  }
  store(seenKey(vault), seen);
}

// ------------------------------------------------------------ forgotten

export interface Forgotten {
  path: string;
  /** When it was last opened or changed. */
  last: number;
}

/** A small, stable hash, so the pick stays the same all day. */
function hash(s: string): number {
  let x = 2166136261;
  for (let i = 0; i < s.length; i++) x = Math.imul(x ^ s.charCodeAt(i), 16777619);
  return x >>> 0;
}

/**
 * Notes not opened or changed in a long time: over a year if there are any,
 * else over six months, else over three. In a stable order for `day`.
 */
export function forgotten(
  rows: Pick<NoteRow, "path" | "modifiedMs">[],
  seen: Record<string, number>,
  opts: { now: number; day: string; skip?: Set<string>; exclude?: (path: string) => boolean },
): Forgotten[] {
  const all = rows
    .filter((r) => !opts.skip?.has(r.path) && !opts.exclude?.(r.path))
    .map((r) => ({ path: r.path, last: Math.max(seen[r.path] ?? 0, r.modifiedMs) }));
  for (const days of [365, 182, 91]) {
    const old = all.filter((x) => opts.now - x.last >= days * DAY);
    if (old.length) {
      const key = (x: Forgotten) => hash(`${opts.day}\u0000${x.path}`);
      return old.sort((a, b) => key(a) - key(b) || a.path.localeCompare(b.path));
    }
  }
  return [];
}

/** "over a year", "8 months". */
export function sinceWords(last: number, now = Date.now()): string {
  const months = Math.floor((now - last) / (30.44 * DAY));
  if (months >= 24) return `${Math.floor(months / 12)} years`;
  if (months >= 12) return "over a year";
  return `${months} months`;
}

// --------------------------------------------------------------- weeks

/** ISO week: [year, week number]. */
export function isoWeek(d: Date): [number, number] {
  const t = new Date(Date.UTC(d.getFullYear(), d.getMonth(), d.getDate()));
  const dow = t.getUTCDay() || 7;
  t.setUTCDate(t.getUTCDate() + 4 - dow);
  const y = t.getUTCFullYear();
  const week = Math.ceil(((t.getTime() - Date.UTC(y, 0, 1)) / DAY + 1) / 7);
  return [y, week];
}

export const weekId = (d: Date) => {
  const [y, w] = isoWeek(d);
  return `${y}-W${String(w).padStart(2, "0")}`;
};

/** Monday 00:00 of `d`'s ISO week. */
export function weekMonday(d: Date): Date {
  const m = new Date(d.getFullYear(), d.getMonth(), d.getDate());
  m.setDate(m.getDate() - ((m.getDay() + 6) % 7));
  return m;
}

// ------------------------------------------------------------------- UI

export interface ResurfaceApp {
  /** Identifies the vault (its folder). */
  vault(): string;
  dailyIndex(): Map<string, string>;
  dailyFolder(): string;
  templateFolder(): string;
  open(path: string): void;
  /** Creates a note (made unique) and returns its path. */
  create(folder: string, name: string, text: string): Promise<string>;
  toast(text: string, kind?: "info" | "error"): void;
}

const skipKey = (vault: string) => `marrow.resurface.skip.${vault}`;
const reviewKey = (vault: string) => `marrow.review.${vault}`;

function excluder(app: ResurfaceApp) {
  const daily = new Set(app.dailyIndex().values());
  const tpl = app.templateFolder().replace(/^\/+|\/+$/g, "");
  return (p: string) => daily.has(p) || (tpl !== "" && p.startsWith(tpl + "/")) || p.startsWith("Reviews/");
}

async function firstWords(path: string, n = 170): Promise<string> {
  const text = await api.readNote(path).catch(() => "");
  // Without the title line, if the note starts with one.
  // Headings ("# Title", "## Log") are left out: the words under them matter.
  const body = text.replace(/^---\n[\s\S]*?\n---\n/, "").replace(/^\s*#{1,6}\s[^\n]*$/gm, "");
  const p = plainText(body);
  return p.length > n ? p.slice(0, n).replace(/\s+\S*$/, "") + "…" : p;
}

let offset = 0;

/** The start screen's "From your archive", "On this day" and weekly review. */
export async function renderResurface(box: HTMLElement, app: ResurfaceApp) {
  const vault = app.vault();
  const now = Date.now();
  const today = isoDay();
  const cards: HTMLElement[] = [];

  // Weekly review, from Friday to Sunday until it's done.
  const dow = new Date().getDay();
  if ((dow === 5 || dow === 6 || dow === 0) && storage<string>(reviewKey(vault), "") !== weekId(new Date())) {
    cards.push(
      h(
        "div.rs-card.rs-review",
        null,
        h("div.rs-k", null, icon("sun-horizon"), "Weekly review"),
        h("p.rs-t", null, "Look back at your week in five minutes: what you wrote, what's still open, and one old note worth another look."),
        h("div.rs-acts", null, h("button.btn.btn-primary", { onclick: () => void openWeeklyReview(app) }, "Start the review")),
      ),
    );
  }

  // From your archive.
  const rows = (await api.queryNotes("", true).catch(() => ({ rows: [] as NoteRow[] }))).rows;
  const skip = new Set(storage<string[]>(skipKey(vault), []));
  const old = forgotten(rows, seenTimes(vault), { now, day: today, skip, exclude: excluder(app) });
  if (old.length) {
    const pick = old[offset % old.length];
    const words = await firstWords(pick.path);
    cards.push(
      h(
        "div.rs-card",
        null,
        h("div.rs-k", null, icon("archive"), "From your archive"),
        h("button.rs-title", { translate: "no", onclick: () => app.open(pick.path) }, displayName(pick.path)),
        h("div.rs-meta", null, `Not opened in ${sinceWords(pick.last, now)}`),
        words ? h("p.rs-t", { translate: "no" }, words) : null,
        h(
          "div.rs-acts",
          null,
          h("button.btn.btn-secondary", { onclick: () => app.open(pick.path) }, "Open"),
          old.length > 1 ? h("button.btn.btn-ghost", { onclick: () => ((offset += 1), void renderResurface(box, app)) }, "Another") : null,
          h(
            "button.btn.btn-ghost",
            {
              title: "Don't suggest this note again",
              onclick: () => {
                store(skipKey(vault), [...skip, pick.path]);
                void renderResurface(box, app);
              },
            },
            "Not this one",
          ),
        ),
      ),
    );
  }

  // On this day.
  const days = app.dailyIndex();
  const earlier = onThisDay(days.keys(), today).slice(0, 3);
  if (earlier.length) {
    const items = await Promise.all(earlier.map(async (d) => ({ day: d, path: days.get(d)!, words: await firstWords(days.get(d)!, 110) })));
    cards.push(
      h(
        "div.rs-card",
        null,
        h("div.rs-k", null, icon("calendar-dots"), "On this day"),
        ...items.map((x) =>
          h(
            "button.rs-day",
            { onclick: () => app.open(x.path) },
            h("span.y", null, `${Number(today.slice(0, 4)) - Number(x.day.slice(0, 4)) === 1 ? "A year ago" : `${Number(today.slice(0, 4)) - Number(x.day.slice(0, 4))} years ago`}`),
            h("span.w", { translate: "no" }, x.words || x.day),
          ),
        ),
      ),
    );
  }
  box.replaceChildren(...cards);
}

// -------------------------------------------------------- weekly review

/** The weekly review's numbers, from notes and tasks. */
export function weekSummary(rows: NoteRow[], tasks: TaskItem[], daily: Map<string, string>, from: number, now: number) {
  const dailyPaths = new Set(daily.values());
  const notDaily = rows.filter((r) => !dailyPaths.has(r.path) && !r.path.startsWith("Reviews/"));
  const created = notDaily.filter((r) => r.createdMs >= from).sort((a, b) => b.createdMs - a.createdMs);
  const createdSet = new Set(created.map((r) => r.path));
  const edited = notDaily.filter((r) => r.modifiedMs >= from && !createdSet.has(r.path)).sort((a, b) => b.modifiedMs - a.modifiedMs);
  const fromDay = isoDay(new Date(from));
  const todayDay = isoDay(new Date(now));
  const weekDays = [...daily.keys()].filter((d) => d >= fromDay && d <= todayDay);
  const weekDaily = new Set(weekDays.map((d) => daily.get(d)!));
  const done = tasks.filter((t) => t.done && weekDaily.has(t.path));
  const nextWeek = isoDay(new Date(now + 7 * DAY));
  const overdue = tasks.filter((t) => !t.done && t.due && t.due < todayDay);
  const upcoming = tasks.filter((t) => !t.done && t.due && t.due >= todayDay && t.due <= nextWeek).sort((a, b) => (a.due! < b.due! ? -1 : 1));
  const loose = created.filter((r) => r.backlinks.length === 0);
  return { created, edited, days: weekDays, done, overdue, upcoming, loose };
}

const PROMPTS: [string, string][] = [
  ["Went well", "What went well this week?"],
  ["Learned", "What did you learn, or notice?"],
  ["Next week", "What matters most next week?"],
];

export async function openWeeklyReview(app: ResurfaceApp) {
  const vault = app.vault();
  const now = Date.now();
  const monday = weekMonday(new Date(now));
  const from = monday.getTime();
  const [y, w] = isoWeek(new Date(now));
  const id = weekId(new Date(now));
  const draftKey = `marrow.review.draft.${vault}.${id}`;
  const draft = storage<Record<string, string>>(draftKey, {});
  const [rows, tasks] = await Promise.all([
    api.queryNotes("", true).then((r) => r.rows).catch(() => [] as NoteRow[]),
    api.tasks(true).catch(() => [] as TaskItem[]),
  ]);
  const s = weekSummary(rows, tasks, app.dailyIndex(), from, now);
  const old = forgotten(rows, seenTimes(vault), { now, day: id, skip: new Set(storage<string[]>(skipKey(vault), [])), exclude: excluder(app) }).slice(0, 3);
  const sunday = new Date(from + 6 * DAY);
  const range = `${monday.toLocaleDateString(uiLocale(), { day: "numeric", month: "long" })} – ${sunday.toLocaleDateString(uiLocale(), { day: "numeric", month: "long", year: "numeric" })}`;

  const close = () => {
    wrap.remove();
    document.removeEventListener("keydown", keys, true);
    document.documentElement.toggleAttribute("data-modal", false);
  };
  // Opening a note closes the review; what's typed is kept for later.
  const go = (path: string) => {
    close();
    app.open(path);
  };
  const link = (r: { path: string }) => h("button.rv-link", { translate: "no", onclick: () => go(r.path) }, displayName(r.path));
  const links = (list: { path: string }[], max = 12) =>
    list.length ? h("div.rv-links", null, ...list.slice(0, max).map(link), list.length > max ? h("span.rv-more", null, `+${list.length - max} more`) : null) : null;
  const stat = (n: number, label: string) => h("div.rv-stat", null, h("b", null, String(n)), h("span", null, label));
  const taskLine = (t: TaskItem) => h("button.rv-task", { onclick: () => go(t.path) }, icon("circle"), h("span", { translate: "no" }, t.text), t.due ? h("span.d", null, t.due) : null);

  const answers = PROMPTS.map(([k, q]) => {
    const ta = h("textarea.input.rv-answer", { rows: 2, placeholder: q, spellcheck: true }) as HTMLTextAreaElement;
    ta.value = draft[k] ?? "";
    ta.addEventListener("input", () => {
      draft[k] = ta.value;
      store(draftKey, draft);
    });
    return [k, q, ta] as const;
  });

  const section = (ic: string, title: string, ...kids: (Node | null)[]) => h("section.rv-sec", null, h("h3", null, icon(ic), title), ...kids);
  const body = h(
    "div.rv-body",
    null,
    section(
      "notebook",
      "Your week",
      h("div.rv-stats", null, stat(s.created.length, s.created.length === 1 ? "new note" : "new notes"), stat(s.edited.length, "edited"), stat(s.days.length, s.days.length === 1 ? "daily note" : "daily notes"), stat(s.done.length, "tasks done")),
      s.created.length ? h("div.rv-sub", null, "New") : null,
      links(s.created),
      s.edited.length ? h("div.rv-sub", null, "Worked on") : null,
      links(s.edited),
      !s.created.length && !s.edited.length && !s.days.length ? h("p.rv-quiet", null, "A quiet week. That's fine too.") : null,
    ),
    s.overdue.length || s.upcoming.length
      ? section(
          "check-square",
          "Still open",
          s.overdue.length ? h("div.rv-sub", null, "Past their date") : null,
          ...s.overdue.slice(0, 8).map(taskLine),
          s.upcoming.length ? h("div.rv-sub", null, "Coming up this week") : null,
          ...s.upcoming.slice(0, 8).map(taskLine),
        )
      : null,
    s.loose.length
      ? section("link-break", "Loose ends", h("p.rv-hint", null, "New this week and nothing links to them yet. A link from a related note helps you find them later."), links(s.loose, 8))
      : null,
    old.length
      ? section("archive", "From your archive", h("p.rv-hint", null, "Worth another look?"), ...old.map((f) => h("div.rv-old", null, link(f), h("span", null, `not opened in ${sinceWords(f.last, now)}`))))
      : null,
    section("pencil-simple-line", "Reflect", h("p.rv-hint", null, "A line or two each. Skip any you like."), ...answers.map(([, q, ta]) => h("label.rv-q", null, h("span", null, q), ta))),
  );

  const save = async () => {
    const list = (rs: { path: string }[]) => rs.map((r) => `[[${displayName(r.path)}]]`).join(", ");
    const parts = [`# Week ${w}, ${y}`, "", `*${range}*`, ""];
    for (const [k, , ta] of answers) if (ta.value.trim()) parts.push(`## ${k}`, "", ta.value.trim(), "");
    parts.push("## The week in notes", "");
    if (s.created.length) parts.push(`- New: ${list(s.created)}`);
    if (s.edited.length) parts.push(`- Worked on: ${list(s.edited)}`);
    parts.push(`- Daily notes: ${s.days.length}`, `- Tasks done: ${s.done.length}`);
    if (s.upcoming.length) parts.push("", "## Coming up", "", ...s.upcoming.map((t) => `- [ ] ${t.text} (${t.due})`));
    try {
      const path = await app.create(`Reviews/${y}`, id, parts.join("\n") + "\n");
      store(reviewKey(vault), id);
      store(draftKey, {});
      close();
      app.open(path);
      app.toast("Week reviewed. See you next week.");
    } catch (e) {
      app.toast(String(e), "error");
    }
  };
  const done = () => {
    store(reviewKey(vault), id);
    close();
    app.toast("Marked as reviewed for this week.");
  };

  const card = h(
    "div.dlg.rv",
    { role: "dialog", "aria-label": "Weekly review" },
    h("div.rv-head", null, h("div", null, h("h2", null, "Weekly review"), h("div.rv-range", null, `Week ${w} · ${range}`)), h("button.icon-btn", { title: "Close (Esc)", "aria-label": "Close", onclick: close }, icon("x"))),
    body,
    h("div.dlg-foot", null, h("button.btn.btn-ghost", { onclick: done }, "Done, without a note"), h("button.btn.btn-primary", { onclick: () => void save() }, icon("floppy-disk"), "Save as a note")),
  );
  const wrap = h("div.backdrop.dlg-wrap", null, card);
  const keys = (e: KeyboardEvent) => {
    if (e.key === "Escape") {
      e.stopPropagation();
      close();
    }
  };
  wrap.addEventListener("mousedown", (e) => e.target === wrap && close());
  document.addEventListener("keydown", keys, true);
  document.body.append(wrap);
  document.documentElement.toggleAttribute("data-modal", true);
}

/** The few days before a stamp, for tests. */
export const _DAY = DAY;
