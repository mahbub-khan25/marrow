import { describe, expect, it } from "vitest";
import type { NoteRow, TaskItem } from "./api";
import { forgotten, isoWeek, sinceWords, weekId, weekMonday, weekSummary } from "./resurface";

const DAY = 86_400_000;
const now = Date.UTC(2026, 9, 25, 12);
const row = (path: string, ageDays: number, createdDays = ageDays, backlinks: string[] = []): NoteRow => ({
  path,
  title: path,
  props: {},
  tags: [],
  modifiedMs: now - ageDays * DAY,
  createdMs: now - createdDays * DAY,
  propLinks: {},
  backlinks,
});

describe("forgotten notes", () => {
  it("prefers notes untouched for over a year, and counts opening as seeing", () => {
    const rows = [row("a.md", 400), row("b.md", 500), row("c.md", 200), row("d.md", 10)];
    const f = forgotten(rows, { "b.md": now - 5 * DAY }, { now, day: "2026-10-25" });
    expect(f.map((x) => x.path)).toEqual(["a.md"]);
  });
  it("falls back to six months, then three", () => {
    const rows = [row("c.md", 200), row("e.md", 100), row("d.md", 10)];
    expect(forgotten(rows, {}, { now, day: "x" }).map((x) => x.path)).toEqual(["c.md"]);
    expect(forgotten([row("e.md", 100)], {}, { now, day: "x" }).map((x) => x.path)).toEqual(["e.md"]);
    expect(forgotten([row("d.md", 10)], {}, { now, day: "x" })).toEqual([]);
  });
  it("skips and excludes, and keeps the same order all day", () => {
    const rows = Array.from({ length: 20 }, (_, i) => row(`n${i}.md`, 400 + i));
    const opts = { now, day: "2026-10-25", skip: new Set(["n3.md"]), exclude: (p: string) => p === "n4.md" };
    const a = forgotten(rows, {}, opts).map((x) => x.path);
    expect(a).toHaveLength(18);
    expect(a).not.toContain("n3.md");
    expect(forgotten(rows, {}, opts).map((x) => x.path)).toEqual(a);
    expect(forgotten(rows, {}, { ...opts, day: "2026-10-26" }).map((x) => x.path)).not.toEqual(a);
  });
  it("says how long", () => {
    expect(sinceWords(now - 400 * DAY, now)).toBe("over a year");
    expect(sinceWords(now - 800 * DAY, now)).toBe("2 years");
    expect(sinceWords(now - 200 * DAY, now)).toBe("6 months");
  });
});

describe("weeks", () => {
  it("numbers ISO weeks", () => {
    expect(isoWeek(new Date(2026, 9, 25))).toEqual([2026, 43]);
    expect(weekId(new Date(2027, 0, 1))).toBe("2026-W53");
    expect(weekId(new Date(2026, 0, 1))).toBe("2026-W01");
    expect(weekMonday(new Date(2026, 9, 25)).getDate()).toBe(19);
  });
  it("sums up a week", () => {
    const from = now - 6 * DAY;
    const rows = [row("new.md", 1, 2), row("linked.md", 1, 3, ["x.md"]), row("old.md", 2, 100), row("ancient.md", 300), row("Daily/2026-10-24.md", 1, 1)];
    const daily = new Map([["2026-10-24", "Daily/2026-10-24.md"], ["2026-10-01", "Daily/2026-10-01.md"]]);
    const t = (text: string, done: boolean, path: string, due: string | null = null): TaskItem => ({ path, name: path, line: 0, done, text, due, fromNote: false });
    const tasks = [t("a", true, "Daily/2026-10-24.md"), t("b", true, "Daily/2026-10-01.md"), t("late", false, "x.md", "2026-10-01"), t("soon", false, "x.md", "2026-10-28"), t("far", false, "x.md", "2026-12-01")];
    const s = weekSummary(rows, tasks, daily, from, now);
    expect(s.created.map((r) => r.path)).toEqual(["new.md", "linked.md"]);
    expect(s.edited.map((r) => r.path)).toEqual(["old.md"]);
    expect(s.days).toEqual(["2026-10-24"]);
    expect(s.done.map((x) => x.text)).toEqual(["a"]);
    expect(s.overdue.map((x) => x.text)).toEqual(["late"]);
    expect(s.upcoming.map((x) => x.text)).toEqual(["soon"]);
    expect(s.loose.map((r) => r.path)).toEqual(["new.md"]);
  });
});
