// The document lifecycle: saving, failed saves, crash drafts and merging.
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const disk = new Map<string, string>();
const drafts = new Map<string, string>();
let failWrites: string | null = null;

vi.mock("./api", () => ({
  displayName: (p: string) => p.replace(/\.md$/, ""),
  api: {
    readNote: vi.fn(async (p: string) => {
      if (!disk.has(p)) throw new Error("missing");
      return disk.get(p)!;
    }),
    writeNote: vi.fn(async (p: string, text: string, expected: string | null) => {
      if (failWrites) throw new Error(failWrites);
      if (expected !== null && expected !== undefined && disk.get(p) !== expected) throw "CHANGED_ON_DISK";
      disk.set(p, text);
      drafts.delete(p);
    }),
    keepDraft: vi.fn(async (p: string, text: string) => {
      drafts.set(p, text);
    }),
    mergeText: vi.fn(async (base: string, mine: string, theirs: string) => ({
      // Good enough for tests: both sides changed different ends.
      text: theirs + mine.slice(base.length),
      conflicts: 0,
    })),
    backupFor: vi.fn(async () => null),
    discardBackup: vi.fn(async () => {}),
  },
}));

import { edit, forget, initDocs, keepDrafts, openDoc, save, saveAll } from "./docs";

const banners: string[] = [];
beforeEach(() => {
  vi.useFakeTimers();
  disk.clear();
  drafts.clear();
  failWrites = null;
  banners.length = 0;
  initDocs({
    onState: () => {},
    banner: (t) => void banners.push(t),
    hideErrorBanner: () => {},
    toast: () => {},
    unlock: async () => false,
  });
});
afterEach(() => {
  vi.useRealTimers();
});

describe("saving", () => {
  it("saves a moment after typing, and says so", async () => {
    disk.set("a.md", "one");
    const d = await openDoc("a.md");
    edit(d, "one two", {});
    expect(d.dirty).toBe(true);
    await vi.advanceTimersByTimeAsync(2100);
    expect(disk.get("a.md")).toBe("one two");
    expect(d.dirty).toBe(false);
    expect(await save(d)).toBe(true);
  });

  it("reports a failed save instead of pretending", async () => {
    disk.set("b.md", "x");
    const d = await openDoc("b.md");
    edit(d, "x y", {});
    failWrites = "The disk is full.";
    expect(await save(d)).toBe(false);
    expect(d.dirty).toBe(true);
    expect(d.state).toBe("error");
    expect(banners.join()).toContain("The disk is full.");
    const left = await saveAll();
    expect(left).toContain(d);
    failWrites = null;
    expect(await save(d)).toBe(true);
    expect((await saveAll()).length).toBe(0);
    forget("b.md");
  });

  it("merges when another app changed the note meanwhile", async () => {
    disk.set("c.md", "start");
    const d = await openDoc("c.md");
    edit(d, "start mine", {});
    disk.set("c.md", "theirs start");
    expect(await save(d)).toBe(true);
    expect(disk.get("c.md")).toBe("theirs start mine");
    forget("c.md");
  });
});

describe("crash drafts", () => {
  it("keeps unsaved text half a second after typing", async () => {
    disk.set("d.md", "old");
    const d = await openDoc("d.md");
    edit(d, "new words", {});
    await vi.advanceTimersByTimeAsync(600);
    expect(drafts.get("d.md")).toBe("new words");
    // A successful save removes the draft.
    await vi.advanceTimersByTimeAsync(2000);
    expect(drafts.has("d.md")).toBe(false);
    forget("d.md");
  });

  it("keeps a draft every few seconds even when typing never pauses", async () => {
    disk.set("g.md", "");
    const d = await openDoc("g.md");
    let text = "";
    // A keystroke every 200 ms for 12 seconds: never a half-second pause.
    for (let t = 0; t < 12000; t += 200) {
      text += "x";
      edit(d, text, {});
      await vi.advanceTimersByTimeAsync(200);
      // Whatever was typed more than 5 seconds ago is safe in a draft.
      if (t >= 5200) expect((drafts.get("g.md") ?? "").length).toBeGreaterThanOrEqual(text.length - 26);
    }
    expect(drafts.get("g.md")?.length).toBeGreaterThan(0);
    forget("g.md");
  });

  it("saves at least every 15 seconds while typing goes on", async () => {
    disk.set("h.md", "");
    const d = await openDoc("h.md");
    let text = "";
    for (let t = 0; t < 16000; t += 300) {
      text += "y";
      edit(d, text, {});
      await vi.advanceTimersByTimeAsync(300);
    }
    expect((disk.get("h.md") ?? "").length).toBeGreaterThan(40);
    forget("h.md");
  });

  it("keeps drafts on request before giving up on a save", async () => {
    disk.set("e.md", "a");
    const d = await openDoc("e.md");
    edit(d, "a b", {});
    failWrites = "locked";
    await save(d);
    drafts.clear();
    expect(await keepDrafts([d])).toEqual([]);
    expect(drafts.get("e.md")).toBe("a b");
    forget("e.md");
  });

  it("forgetting a note stops its pending save and draft", async () => {
    disk.set("f.md", "a");
    const d = await openDoc("f.md");
    edit(d, "a b", {});
    forget("f.md");
    await vi.advanceTimersByTimeAsync(3000);
    expect(disk.get("f.md")).toBe("a");
    expect(drafts.has("f.md")).toBe(false);
  });
});
