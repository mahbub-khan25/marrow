// Canvas saving: conflicts with another app keep every edit, and a canvas
// whose changes can't be saved or kept is never replaced.
import { beforeEach, describe, expect, it, vi } from "vitest";

const disk = new Map<string, string>();
let failWrites = false;
let failDrafts = false;
let onCopy: (() => void) | null = null;
let onWrite: (() => void) | null = null;

vi.mock("@tauri-apps/api/core", () => ({ convertFileSrc: (p: string) => p, invoke: vi.fn() }));
vi.mock("@tauri-apps/plugin-dialog", () => ({ ask: vi.fn(async () => false) }));
vi.mock("@tauri-apps/plugin-opener", () => ({ openUrl: vi.fn() }));
vi.mock("./tree", () => ({ DRAG_TYPE: "x-marrow" }));
vi.mock("./api", () => ({
  displayName: (p: string) => p.split("/").pop()!.replace(/\.(md|canvas)$/, ""),
  parentOf: (p: string) => (p.includes("/") ? p.slice(0, p.lastIndexOf("/")) : ""),
  api: {
    readNote: vi.fn(async (p: string) => disk.get(p) ?? ""),
    writeNote: vi.fn(async (p: string, text: string, expected?: string) => {
      if (failWrites) throw new Error("The disk is full.");
      if (expected !== undefined && disk.get(p) !== expected) throw "CHANGED_ON_DISK";
      disk.set(p, text);
      // The person keeps drawing while the save is writing.
      const f = onWrite;
      onWrite = null;
      f?.();
    }),
    createFile: vi.fn(async (folder: string, name: string, ext: string, text: string) => {
      const p = `${folder ? folder + "/" : ""}${name}.${ext}`;
      disk.set(p, text);
      // The person keeps drawing while the copy is being made.
      onCopy?.();
      return p;
    }),
    keepDraft: vi.fn(async () => {
      if (failDrafts) throw new Error("no space");
    }),
    backupFor: vi.fn(async () => null),
    discardBackup: vi.fn(async () => {}),
  },
}));

import { CanvasView } from "./canvas";

const empty = JSON.stringify({ nodes: [], edges: [] });
const toasts: string[] = [];
let view: CanvasView;

function addCard(id: string) {
  // What a click on the canvas does: change the data, then record it.
  const v = view as unknown as { data: { nodes: object[] }; changed(): void };
  v.data.nodes.push({ id, type: "text", text: id, x: 0, y: 0, width: 100, height: 60 });
  v.changed();
}

beforeEach(() => {
  disk.clear();
  failWrites = failDrafts = false;
  onCopy = null;
  onWrite = null;
  toasts.length = 0;
  window.requestAnimationFrame = (f: FrameRequestCallback) => (f(0), 0);
  view = new CanvasView(document.createElement("div"), { vaultRoot: () => "/v", openPath: () => {}, toast: (t) => void toasts.push(t) });
});

describe("canvas saving", () => {
  it("keeps both versions, including edits made during the conflict", async () => {
    disk.set("Board.canvas", empty);
    await view.load("Board.canvas");
    addCard("mine-1");
    // Another app changes the canvas.
    disk.set("Board.canvas", JSON.stringify({ nodes: [{ id: "theirs", type: "text", text: "t", x: 0, y: 0, width: 1, height: 1 }], edges: [] }));
    onCopy = () => {
      onCopy = null;
      addCard("mine-2");
    };
    expect(await view.flush()).toBe(true);
    const copy = disk.get("Board (my changes).canvas")!;
    expect(copy).toContain("mine-1");
    expect(copy).toContain("mine-2");
    // The canvas now shows the other app's version; nothing was lost.
    expect(disk.get("Board.canvas")).toContain("theirs");
    expect(toasts.join()).toContain("Board (my changes)");
  });

  it("stays on a canvas whose changes can't be saved or kept", async () => {
    disk.set("A.canvas", empty);
    disk.set("B.canvas", empty);
    await view.load("A.canvas");
    addCard("unsaved");
    failWrites = failDrafts = true;
    await expect(view.load("B.canvas")).rejects.toThrow("stays open");
    expect(view.path).toBe("A.canvas");
    // Once the changes are kept safe, switching works.
    failDrafts = false;
    await view.load("B.canvas");
    expect(view.path).toBe("B.canvas");
  });

  it("saves edits made while a save is writing before reporting success", async () => {
    disk.set("A.canvas", empty);
    disk.set("B.canvas", empty);
    await view.load("A.canvas");
    addCard("first");
    onWrite = () => addCard("during-save");
    expect(await view.flush()).toBe(true);
    expect(disk.get("A.canvas")).toContain("during-save");
    await view.load("B.canvas");
    expect(disk.get("A.canvas")).toContain("during-save");
  });

  it("reports changes that could be neither saved nor kept", async () => {
    disk.set("A.canvas", empty);
    await view.load("A.canvas");
    addCard("unsaved");
    failWrites = true;
    expect(await view.flush()).toBe(false);
    expect(view.changesAtRisk).toBe(false); // kept as a draft
    failDrafts = true;
    expect(await view.flush()).toBe(false);
    expect(view.changesAtRisk).toBe(true);
  });
});
