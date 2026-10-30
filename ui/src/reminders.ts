// Task reminders in the window: the picker ("Remind me"), and the cards
// that appear when one is due, with Done, Snooze and Open. The app (Rust)
// rings them; the note keeps the time as `⏰ 2026-11-03 09:00`.

import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

import { api } from "./api";
import { editNote } from "./database";
import { findDoc } from "./docs";
import { h, icon, isoDay } from "./util";

export interface DueReminder {
  id: string;
  path: string;
  name: string;
  line: number;
  text: string;
  at: string;
}

export interface ReminderApp {
  openNote(path: string, at?: { line: number; ch: number }): void;
  toast(text: string, kind?: "info" | "error"): void;
}

const SNOOZE_MINUTES = 10;

/** `YYYY-MM-DD HH:MM` for a local date. */
export function stamp(d: Date): string {
  const p = (n: number) => String(n).padStart(2, "0");
  return `${isoDay(d)} ${p(d.getHours())}:${p(d.getMinutes())}`;
}

/** "Today, 09:00" / "Tomorrow, 18:00" / "Tue 3 Nov, 09:00". */
export function whenLabel(at: string): string {
  const d = new Date(at.replace(" ", "T"));
  if (isNaN(d.getTime())) return at;
  const today = isoDay();
  const tomorrow = isoDay(new Date(Date.now() + 86_400_000));
  const time = d.toLocaleTimeString(undefined, {
    hour: "2-digit",
    minute: "2-digit",
  });
  const day = at.slice(0, 10);
  if (day === today) return `Today, ${time}`;
  if (day === tomorrow) return `Tomorrow, ${time}`;
  const sameYear = day.slice(0, 4) === today.slice(0, 4);
  return `${d.toLocaleDateString(undefined, { weekday: "short", day: "numeric", month: "short", year: sameYear ? undefined : "numeric" })}, ${time}`;
}

/** Finds the task's line again (by its text, if lines have moved). */
async function locate(
  lines: string[],
  line: number,
  text: string,
): Promise<number> {
  const is = async (i: number) =>
    i >= 0 &&
    i < lines.length &&
    (await invoke<string | null>("task_text", { line: lines[i] })) === text;
  if (await is(line)) return line;
  for (let i = 0; i < lines.length; i++) if (await is(i)) return i;
  return -1;
}

/** Changes one task line (found again by its text if lines have moved). */
async function changeTask(
  path: string,
  line: number,
  text: string,
  change: (l: string) => Promise<string>,
) {
  // The open copy if the note is open (it may have unsaved typing), else the file.
  const original = findDoc(path)?.text ?? (await api.readNote(path));
  const lines = original.split("\n");
  const i = await locate(lines, line, text);
  if (i < 0) throw new Error("That task isn't in the note any more.");
  const next = await change(lines[i]);
  await editNote(path, (cur) => {
    const ls = cur.split("\n");
    // Only if that line is still the same (nothing typed into it meanwhile).
    if (ls[i] === lines[i]) ls[i] = next;
    return ls.join("\n");
  });
}

/** Sets (or with null removes) the reminder on a task in a note. */
export const setReminder = (
  path: string,
  line: number,
  text: string,
  at: string | null,
) =>
  changeTask(path, line, text, (l) =>
    invoke<string>("reminder_line", { line: l, at }),
  );

/** Ticks a task done. */
const complete = (path: string, line: number, text: string) =>
  changeTask(path, line, text, async (l) => l.replace("[ ]", "[x]"));

/** The reminder written on an editor line (`YYYY-MM-DD HH:MM`), if any. */
export const reminderOf = (line: string) =>
  invoke<string | null>("reminder_of", { line });

/** A line with its reminder set or removed (made a task first if it isn't one). */
export async function lineWithReminder(
  line: string,
  at: string | null,
): Promise<string> {
  const isTask = /^\s*(?:>\s*)*(?:[-*+]|\d+[.)]) \[[ xX]\] /.test(line);
  const indent = line.match(/^\s*/)?.[0] ?? "";
  const base = isTask
    ? line
    : `${indent}- [ ] ${line.slice(indent.length).replace(/^[-*+] /, "")}`;
  return invoke<string>("reminder_line", { line: base, at });
}

/**
 * The "Remind me" picker, beside `anchor`: soon, this evening, tomorrow
 * morning, any date and time, or remove.
 */
export function pickReminder(
  anchor: HTMLElement,
  current: string | null,
  onPick: (at: string | null) => void,
) {
  document.querySelector(".remind-pop")?.remove();
  const now = new Date();
  const inHour = new Date(now.getTime() + 3_600_000);
  const evening = new Date(now);
  evening.setHours(18, 0, 0, 0);
  const tomorrow = new Date(now.getTime() + 86_400_000);
  tomorrow.setHours(9, 0, 0, 0);
  const pick = (at: string | null) => {
    pop.remove();
    onPick(at);
  };
  const opt = (ic: string, label: string, at: Date) =>
    h(
      "button.remind-opt",
      { onclick: () => pick(stamp(at)) },
      icon(ic),
      h("span", null, label),
      h("span.remind-when", null, whenLabel(stamp(at))),
    );
  const input = h("input.input", {
    type: "datetime-local",
    value: (current ?? stamp(tomorrow)).replace(" ", "T"),
    "aria-label": "Date and time",
  }) as HTMLInputElement;
  const set = h(
    "button.btn.btn-primary",
    {
      onclick: () =>
        input.value && pick(input.value.replace("T", " ").slice(0, 16)),
    },
    "Set",
  );
  const pop = h(
    "div.remind-pop",
    { role: "dialog", "aria-label": "Remind me" },
    h(
      "div.remind-title",
      null,
      current ? `Reminder: ${whenLabel(current)}` : "Remind me",
    ),
    opt("clock", "In an hour", inHour),
    evening.getTime() > now.getTime() + 15 * 60_000
      ? opt("sun-horizon", "This evening", evening)
      : null,
    opt("sun", "Tomorrow morning", tomorrow),
    h("div.remind-custom", null, input, set),
    current
      ? h(
          "button.remind-opt.remind-remove",
          { onclick: () => pick(null) },
          icon("bell-slash"),
          h("span", null, "Remove the reminder"),
        )
      : null,
  );
  document.body.append(pop);
  const r = anchor.getBoundingClientRect();
  const w = 300;
  pop.style.left = `${Math.max(8, Math.min(window.innerWidth - w - 8, r.left))}px`;
  pop.style.top = `${Math.min(window.innerHeight - 280, r.bottom + 6)}px`;
  const close = (e: Event) => {
    if (
      e instanceof KeyboardEvent
        ? e.key === "Escape"
        : !pop.contains(e.target as Node)
    ) {
      pop.remove();
      document.removeEventListener("mousedown", close, true);
      document.removeEventListener("keydown", close, true);
    }
  };
  window.setTimeout(() => {
    document.addEventListener("mousedown", close, true);
    document.addEventListener("keydown", close, true);
  }, 0);
  (pop.querySelector("button") as HTMLElement | null)?.focus();
}

/** Shows due reminders as cards in the corner, until handled. */
export function startReminders(app: ReminderApp) {
  const stack = h("div.reminders", {
    role: "region",
    "aria-label": "Reminders",
  });
  document.body.append(stack);
  void listen<DueReminder>("reminder", (e) => {
    const r = e.payload;
    stack.querySelector(`[data-id="${r.id}"]`)?.remove();
    const card: HTMLElement = h(
      "div.reminder-card",
      { "data-id": r.id, role: "alert" },
      h(
        "div.reminder-head",
        null,
        icon("bell-ringing"),
        h("span", null, whenLabel(r.at)),
      ),
      h("div.reminder-text", { translate: "no" }, r.text),
      h("div.reminder-note", { translate: "no" }, r.name),
      h(
        "div.reminder-actions",
        null,
        h(
          "button.btn.btn-primary",
          {
            onclick: () => act(() => complete(r.path, r.line, r.text), "Done."),
          },
          icon("check"),
          "Done",
        ),
        h(
          "button.btn.btn-secondary",
          {
            onclick: () =>
              act(
                () =>
                  setReminder(
                    r.path,
                    r.line,
                    r.text,
                    stamp(new Date(Date.now() + SNOOZE_MINUTES * 60_000)),
                  ),
                `Snoozed for ${SNOOZE_MINUTES} minutes.`,
              ),
          },
          `Snooze ${SNOOZE_MINUTES} min`,
        ),
        h(
          "button.btn.btn-ghost",
          {
            onclick: () => (
              app.openNote(r.path, { line: r.line, ch: 0 }),
              card.remove()
            ),
          },
          "Open",
        ),
      ),
    );
    const act = async (fn: () => Promise<void>, msg: string) => {
      try {
        await fn();
        card.remove();
        app.toast(msg);
      } catch (err) {
        app.toast(String(err instanceof Error ? err.message : err), "error");
      }
    };
    stack.append(card);
  });
}
