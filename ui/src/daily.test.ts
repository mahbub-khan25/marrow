import { describe, expect, it } from "vitest";
import { dailyIndex, dailyPathFor, dayOf, isDay, misplaced, monthGrid, onThisDay } from "./daily";

describe("daily notes", () => {
  it("files a day under its year and month", () => {
    expect(dailyPathFor("Daily", "2026-10-24", "year-month")).toBe("Daily/2026/10 October/2026-10-24.md");
    expect(dailyPathFor("/Journal/", "2027-01-03", "year-month")).toBe("Journal/2027/01 January/2027-01-03.md");
    expect(dailyPathFor("", "2026-10-24", "year-month")).toBe("2026/10 October/2026-10-24.md");
    expect(dailyPathFor("Daily", "2026-10-24", "flat")).toBe("Daily/2026-10-24.md");
  });

  it("only takes real dates inside the daily folder", () => {
    expect(isDay("2026-02-29")).toBe(false);
    expect(isDay("2028-02-29")).toBe(true);
    expect(dayOf("Daily/2026/10 October/2026-10-24.md", "Daily")).toBe("2026-10-24");
    expect(dayOf("Daily/2026-10-24.md", "Daily")).toBe("2026-10-24");
    expect(dayOf("Projects/2026-10-24.md", "Daily")).toBeNull();
    expect(dayOf("Daily/2026-10-24.canvas", "Daily")).toBeNull();
    expect(dayOf("Daily/Meeting 2026-10-24.md", "Daily")).toBeNull();
  });

  it("finds old flat notes and new nested ones together, in date order", () => {
    const files = ["Daily/2026/10 October/2026-10-24.md", "Daily/2025-01-02.md", "Daily/2026-03-05.md", "Daily/notes.md", "Other/2026-01-01.md"];
    expect([...dailyIndex(files, "Daily").keys()]).toEqual(["2025-01-02", "2026-03-05", "2026-10-24"]);
  });

  it("lists notes that need moving, and leaves those already in place", () => {
    const files = ["Daily/2025-01-02.md", "Daily/2026/10 October/2026-10-24.md"];
    expect(misplaced(files, "Daily", "year-month")).toEqual([["Daily/2025-01-02.md", "Daily/2025/01 January"]]);
    expect(misplaced(files, "Daily", "flat")).toEqual([["Daily/2026/10 October/2026-10-24.md", "Daily"]]);
  });

  it("finds the same day in earlier years, newest first", () => {
    const days = ["2023-10-24", "2025-10-24", "2026-10-24", "2025-10-23", "2027-10-24"];
    expect(onThisDay(days, "2026-10-24")).toEqual(["2025-10-24", "2023-10-24"]);
    expect(onThisDay(["2027-02-28", "2024-02-29"], "2028-02-29")).toEqual(["2027-02-28", "2024-02-29"]);
  });

  it("draws six weeks for any month, from the chosen first weekday", () => {
    const sun = monthGrid(2026, 9, 0); // October 2026 starts on a Thursday
    expect(sun).toHaveLength(42);
    expect(sun[0].day).toBe("2026-09-27");
    expect(sun[4]).toEqual({ day: "2026-10-01", inMonth: true });
    const mon = monthGrid(2026, 9, 1);
    expect(mon[0].day).toBe("2026-09-28");
    expect(mon.filter((d) => d.inMonth)).toHaveLength(31);
  });
});
