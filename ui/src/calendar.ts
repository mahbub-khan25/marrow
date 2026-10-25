// The Calendar view of the sidebar: a month (or a whole year) of daily notes,
// open tasks by due date, and "On this day" from earlier years.

import { api, type TaskItem } from "./api";
import { MONTHS, monthGrid, onThisDay, weekStart, ymd } from "./daily";
import { $, h, icon, isoDay, longDate, storage, store } from "./util";

export interface CalendarApp {
  /** Daily notes, keyed by YYYY-MM-DD. */
  dailyIndex(): Map<string, string>;
  /** Opens (creating if needed) the note for a day. */
  openDay(day: string, newTab: boolean): void;
  openNote(path: string, newTab: boolean): void;
  /** How many daily notes are outside their year/month folder (0 when not using folders). */
  misplacedCount(): number;
  layout(): "year-month" | "flat";
  /** Moves them into their year/month folders. */
  organize(): Promise<void>;
}

/** The first line of text in a note, without Markdown marks. */
function firstLine(text: string): string {
  const body = text.replace(/^---\n[\s\S]*?\n---\n?/, "");
  for (const raw of body.split("\n")) {
    const l = raw
      .replace(/^\s*(#{1,6}\s+|[-*+]\s+(\[[ xX]\]\s+)?|>\s*|\d+[.)]\s+)/, "")
      .replace(/!?\[\[([^\]|]+)(?:\|([^\]]+))?\]\]/g, (_m, a: string, b?: string) => b ?? a)
      .replace(/(\*\*|__|==|~~|`)/g, "")
      .trim();
    if (l && !/^(Log|Tasks)$/i.test(l)) return l;
  }
  return "";
}

export class CalendarView {
  private year: number;
  private month: number;
  private mode: "month" | "year" = storage<"month" | "year">("marrow.calendar.mode", "month");
  private selected = isoDay();
  private tasks = new Map<string, number>();
  private seq = 0;
  private start = weekStart();

  constructor(private app: CalendarApp) {
    const now = new Date();
    this.year = now.getFullYear();
    this.month = now.getMonth();
  }

  /** Shows a day's month and selects it (e.g. when a daily note opens). */
  reveal(day: string) {
    this.selected = day;
    this.year = Number(day.slice(0, 4));
    this.month = Number(day.slice(5, 7)) - 1;
    this.mode = "month";
    void this.refresh();
  }

  /** Redraws, re-reading tasks. */
  async refresh() {
    const mine = ++this.seq;
    const all = await api.tasks(false).catch(() => [] as TaskItem[]);
    if (mine !== this.seq) return;
    this.tasks.clear();
    for (const t of all) if (t.due) this.tasks.set(t.due, (this.tasks.get(t.due) ?? 0) + 1);
    this.render();
    void this.renderOnThisDay(mine);
  }

  private go(delta: number) {
    if (this.mode === "year") this.year += delta;
    else {
      const d = new Date(this.year, this.month + delta, 1);
      this.year = d.getFullYear();
      this.month = d.getMonth();
    }
    this.render();
  }

  private today() {
    this.reveal(isoDay());
  }

  private setMode(m: "month" | "year") {
    this.mode = m;
    store("marrow.calendar.mode", m);
    this.render();
  }

  private render() {
    const notes = this.app.dailyIndex();
    const title =
      this.mode === "year"
        ? h("button.cal-title", { title: "Show the month", onclick: () => this.setMode("month") }, String(this.year))
        : h("button.cal-title", { title: "Show the whole year", onclick: () => this.setMode("year") }, `${MONTHS[this.month]} ${this.year}`);
    const head = h(
      "div.cal-head",
      null,
      h("button.icon-btn", { title: this.mode === "year" ? "Previous year" : "Previous month", onclick: () => this.go(-1) }, icon("caret-left")),
      title,
      h("button.icon-btn", { title: this.mode === "year" ? "Next year" : "Next month", onclick: () => this.go(1) }, icon("caret-right")),
      h("button.btn.btn-secondary.cal-today", { onclick: () => this.today() }, "Today"),
    );
    const body = this.mode === "year" ? this.yearView(notes) : this.monthView(notes);
    const banner = this.banner();
    $("cal-main").replaceChildren(...[head, body, banner].filter((x): x is HTMLElement => !!x));
  }

  private monthView(notes: Map<string, string>): HTMLElement {
    const today = isoDay();
    // Weekday initials in the computer's language, from the chosen first day.
    const sunday = new Date(2026, 0, 4);
    const names = Array.from({ length: 7 }, (_, i) => {
      const d = new Date(sunday);
      d.setDate(4 + ((this.start + i) % 7));
      return d.toLocaleDateString(undefined, { weekday: "narrow" });
    });
    const grid = h("div.cal-grid", { role: "grid" }, ...names.map((n) => h("div.cal-wd", null, n)));
    for (const { day, inMonth } of monthGrid(this.year, this.month, this.start)) {
      const note = notes.get(day);
      const due = this.tasks.get(day) ?? 0;
      const tip = [longDate(day), note ? "Daily note" : "No note yet: click to start one", due ? `${due} open task${due === 1 ? "" : "s"} due` : ""].filter(Boolean).join(" · ");
      grid.append(
        h(
          "button.cal-day",
          {
            class: ["cal-day", inMonth ? "" : "is-out", day === today ? "is-today" : "", day === this.selected ? "is-selected" : "", note ? "has-note" : ""].filter(Boolean).join(" "),
            title: tip,
            onclick: (e: MouseEvent) => this.pick(day, e.ctrlKey || e.metaKey),
          },
          String(Number(day.slice(8))),
          h("span.cal-marks", null, note ? h("span.cal-dot") : null, due ? h("span.cal-dot.is-task") : null),
        ),
      );
    }
    const count = [...notes.keys()].filter((d) => d.startsWith(ymd(this.year, this.month, 1).slice(0, 7))).length;
    return h("div.cal-month", null, grid, h("div.side-note.cal-sum", null, count ? `${count} daily note${count === 1 ? "" : "s"} this month` : "No daily notes this month yet."));
  }

  private yearView(notes: Map<string, string>): HTMLElement {
    const now = isoDay();
    return h(
      "div.cal-year",
      null,
      ...MONTHS.map((name, m) => {
        const prefix = ymd(this.year, m, 1).slice(0, 7);
        const n = [...notes.keys()].filter((d) => d.startsWith(prefix)).length;
        // A small heat strip: one cell per day, filled where there's a note.
        const days = new Date(this.year, m + 1, 0).getDate();
        const strip = h("div.cal-strip", null, ...Array.from({ length: days }, (_, i) => h("span", { class: notes.has(ymd(this.year, m, i + 1)) ? "on" : "" })));
        return h(
          "button.cal-mon",
          {
            class: ["cal-mon", now.startsWith(prefix) ? "is-now" : ""].filter(Boolean).join(" "),
            onclick: () => {
              this.month = m;
              this.setMode("month");
            },
          },
          h("span.cal-mon-name", null, name.slice(0, 3)),
          h("span.cal-mon-n", null, n ? String(n) : "–"),
          strip,
        );
      }),
    );
  }

  private banner(): HTMLElement | null {
    const n = this.app.misplacedCount();
    if (!n) return null;
    const btn = h("button.btn.btn-secondary", null, this.app.layout() === "year-month" ? "Organize them" : "Move them back") as HTMLButtonElement;
    btn.onclick = async () => {
      btn.disabled = true;
      btn.textContent = "Organizing…";
      await this.app.organize();
      void this.refresh();
    };
    return h(
      "div.cal-banner",
      null,
      h(
        "p",
        null,
        this.app.layout() === "year-month"
          ? `${n} daily note${n === 1 ? " is" : "s are"} not in year and month folders yet. Organizing keeps every link working.`
          : `${n} daily note${n === 1 ? " is" : "s are"} in year and month folders, but your settings keep daily notes in one folder.`,
      ),
      btn,
    );
  }

  private pick(day: string, newTab: boolean) {
    this.selected = day;
    this.render();
    void this.renderOnThisDay(this.seq);
    this.app.openDay(day, newTab);
  }

  private async renderOnThisDay(mine: number) {
    const notes = this.app.dailyIndex();
    const days = onThisDay(notes.keys(), this.selected).slice(0, 10);
    const box = $("cal-otd");
    const when = this.selected === isoDay() ? "today" : new Date(`${this.selected}T12:00:00`).toLocaleDateString(undefined, { day: "numeric", month: "long" });
    const head = h("div.smallcaps", null, `On this day · ${when}`);
    if (!days.length) {
      box.replaceChildren(head, h("p.side-note", null, "Notes from this date in earlier years will show up here."));
      return;
    }
    const texts = await Promise.all(days.map((d) => api.readNote(notes.get(d)!).catch(() => "")));
    if (mine !== this.seq) return;
    const thisYear = Number(this.selected.slice(0, 4));
    box.replaceChildren(
      head,
      ...days.map((d, i) => {
        const years = thisYear - Number(d.slice(0, 4));
        const label = years === 1 ? "1 year ago" : `${years} years ago`;
        return h(
          "button.note-row",
          { onclick: (e: MouseEvent) => this.app.openNote(notes.get(d)!, e.ctrlKey || e.metaKey) },
          h("span.t", null, `${label} · ${d.slice(0, 4)}`),
          h("span.f", null, firstLine(texts[i]) || "Empty note"),
        );
      }),
    );
  }
}
