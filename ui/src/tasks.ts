// The Tasks view: every to-do in the vault in one list, by when it's due.
// Ticking one here ticks it in its note. Due dates come from
// `📅 2026-10-20`, `due: 2026-10-20`, a [[2026-10-20]] link, or the daily
// note a task is in.

import { api, type TaskItem } from "./api";
import { editNote } from "./database";
import { $, h, icon, isoDay, storage, store } from "./util";

export interface TasksApp {
  openNote(path: string, at?: { line: number; ch: number; len?: number }, newTab?: boolean): void;
  /** Adds `- [ ] text` to today's note. */
  addTask(text: string): Promise<void>;
  toast(text: string, kind?: "info" | "error"): void;
}

/** Ticks or unticks the task on `line`. */
export async function toggleTaskAt(path: string, line: number) {
  await editNote(path, (text) => {
    const lines = text.split("\n");
    const l = lines[line];
    if (l === undefined) return text;
    lines[line] = l.replace(/^(\s*(?:>\s*)*(?:[-*+]|\d+[.)])\s+\[)([ xX])(\])/, (_m, a: string, c: string, b: string) => a + (c === " " ? "x" : " ") + b);
    return lines.join("\n");
  });
}

const addDays = (day: string, n: number) => {
  const d = new Date(`${day}T12:00:00`);
  d.setDate(d.getDate() + n);
  return isoDay(d);
};

function dueLabel(due: string, today: string): string {
  if (due === today) return "Today";
  if (due === addDays(today, 1)) return "Tomorrow";
  if (due === addDays(today, -1)) return "Yesterday";
  const d = new Date(`${due}T12:00:00`);
  const sameYear = due.slice(0, 4) === today.slice(0, 4);
  return d.toLocaleDateString(undefined, { weekday: due < addDays(today, 7) && due > today ? "long" : undefined, day: "numeric", month: "short", year: sameYear ? undefined : "numeric" });
}

/** Task text without Markdown marks, for a compact list. */
const plain = (s: string) =>
  s
    .replace(/!?\[\[([^\]|]+)(?:\|([^\]]+))?\]\]/g, (_m, a: string, b?: string) => b ?? a.split("/").pop()!)
    .replace(/\[([^\]]+)\]\([^)]*\)/g, "$1")
    .replace(/(\*\*|__|==|~~|`)/g, "");

export class TasksView {
  private showDone = storage<boolean>("marrow.tasks.done", false);
  private seq = 0;

  constructor(private app: TasksApp) {
    const input = $<HTMLInputElement>("task-input");
    input.addEventListener("keydown", async (e) => {
      if (e.key !== "Enter" || !input.value.trim()) return;
      e.preventDefault();
      const text = input.value.trim();
      input.value = "";
      await this.app.addTask(text);
      void this.refresh();
    });
    const done = $<HTMLInputElement>("task-show-done");
    done.checked = this.showDone;
    done.addEventListener("change", () => {
      this.showDone = done.checked;
      store("marrow.tasks.done", this.showDone);
      void this.refresh();
    });
  }

  focus() {
    $<HTMLInputElement>("task-input").focus();
  }

  async refresh() {
    const mine = ++this.seq;
    const all = await api.tasks(this.showDone).catch(() => [] as TaskItem[]);
    if (mine !== this.seq) return;
    const today = isoDay();
    const open = all.filter((t) => !t.done);
    const groups: [string, string, TaskItem[]][] = [
      ["Overdue", "warning-circle", open.filter((t) => t.due && t.due < today && !t.fromNote)],
      ["Today", "sun", open.filter((t) => t.due === today)],
      ["Upcoming", "calendar-blank", open.filter((t) => t.due && t.due > today)],
      // Unticked tasks left behind in earlier daily notes.
      ["From earlier days", "clock-counter-clockwise", open.filter((t) => t.due && t.due < today && t.fromNote)],
      ["No date", "list-checks", open.filter((t) => !t.due)],
      ["Done", "check-circle", all.filter((t) => t.done)],
    ];
    for (const [, , list] of groups) list.sort((a, b) => (a.due ?? "9999").localeCompare(b.due ?? "9999") || a.name.localeCompare(b.name) || a.line - b.line);
    $("task-summary").textContent = open.length ? `${open.length} open ${open.length === 1 ? "task" : "tasks"}` : "No open tasks. Type one above, or write - [ ] in any note.";
    const box = $("task-list");
    box.replaceChildren(
      ...groups
        .filter(([, , list]) => list.length)
        .map(([title, ic, list]) => {
          const shown = title === "Done" ? list.slice(0, 50) : list;
          return h(
            "section.task-group",
            null,
            h("div.task-group-head", null, icon(ic), h("span", null, title), h("span.count", null, String(list.length))),
            ...shown.map((t) => this.item(t, today)),
          );
        }),
    );
  }

  private item(t: TaskItem, today: string): HTMLElement {
    const check = h(
      "button.task-check",
      {
        role: "checkbox",
        "aria-checked": String(t.done),
        "aria-label": t.done ? "Mark as not done" : "Mark as done",
        onclick: async (e: Event) => {
          e.stopPropagation();
          row.classList.toggle("is-done", !t.done);
          try {
            await toggleTaskAt(t.path, t.line);
          } catch (err) {
            this.app.toast(`Couldn't change the task: ${err}`, "error");
          }
          window.setTimeout(() => void this.refresh(), 250);
        },
      },
      icon(t.done ? "check-square" : "square"),
    );
    const meta = h("span.task-meta", null, t.name);
    if (t.due && !t.fromNote) meta.append(" · ", h(`span.task-due${t.due < today && !t.done ? ".late" : ""}` as "span", null, dueLabel(t.due, today)));
    const row = h(
      `div.task${t.done ? ".is-done" : ""}` as "div",
      {
        title: `${t.path}, line ${t.line + 1}`,
        onclick: (e: MouseEvent) => this.app.openNote(t.path, { line: t.line, ch: 0 }, e.ctrlKey || e.metaKey),
      },
      check,
      h("span.task-body", null, h("span.task-text", null, plain(t.text)), meta),
    );
    return row;
  }
}
