// Reminder times: written as local `YYYY-MM-DD HH:MM`, shown as "Today, …".
import { describe, expect, it, vi } from "vitest";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(),
  convertFileSrc: (p: string) => p,
}));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn() }));

import { stamp, whenLabel } from "./reminders";

describe("reminder times", () => {
  it("writes local times", () => {
    expect(stamp(new Date(2026, 10, 3, 9, 5))).toBe("2026-11-03 09:05");
  });

  it("says today and tomorrow", () => {
    const now = new Date();
    now.setHours(9, 0, 0, 0);
    expect(whenLabel(stamp(now))).toMatch(/^Today, /);
    const next = new Date(now.getTime() + 86_400_000);
    expect(whenLabel(stamp(next))).toMatch(/^Tomorrow, /);
    expect(whenLabel("not a time")).toBe("not a time");
  });
});
