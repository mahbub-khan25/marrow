// Home board: template files (what's shared and what isn't), new cards and day labels.
import { describe, expect, it, vi } from "vitest";

vi.mock("@tauri-apps/api/core", () => ({ convertFileSrc: (p: string) => p, invoke: vi.fn() }));
vi.mock("@tauri-apps/plugin-dialog", () => ({ ask: vi.fn(async () => false), save: vi.fn(async () => null) }));
vi.mock("@tauri-apps/plugin-opener", () => ({ openUrl: vi.fn() }));
vi.mock("./tree", () => ({ DRAG_TYPE: "x-marrow" }));
vi.mock("./preview", () => ({ renderMarkdown: () => "" }));
vi.mock("./resurface", () => ({ renderResurface: vi.fn(async () => {}) }));
vi.mock("./tasks", () => ({ toggleTaskAt: vi.fn() }));
vi.mock("./api", () => ({
  displayName: (p: string) => p.split("/").pop()!.replace(/\.(md|canvas)$/, ""),
  parentOf: (p: string) => (p.includes("/") ? p.slice(0, p.lastIndexOf("/")) : ""),
  api: {},
}));

import { dayLabel, makeCard, readTemplate, starterBoard, writeTemplate } from "./home";

describe("home templates", () => {
  it("leave sticky words out unless asked, and read back with new ids", () => {
    const sticky = makeCard("sticky", { x: 10, y: 20 }, { text: "My bank PIN hint" });
    const tasks = makeCard("tasks", { x: 300, y: 20 });
    const board = { nodes: [sticky, tasks], edges: [{ id: "e1", fromNode: sticky.id, toNode: tasks.id }], home: { bg: "cork" } };

    const bare = JSON.parse(writeTemplate(board, "Mine", false));
    expect(bare.marrowHome).toBe(1);
    expect(bare.name).toBe("Mine");
    expect(bare.board.nodes[0].text).toBe("");
    expect(board.nodes[0].text).toBe("My bank PIN hint"); // the board itself is untouched

    const back = readTemplate(writeTemplate(board, "Mine", true));
    expect(back.name).toBe("Mine");
    expect(back.board.nodes[0].text).toBe("My bank PIN hint");
    expect(back.board.nodes[0].id).not.toBe(sticky.id);
    expect(back.board.edges[0].fromNode).toBe(back.board.nodes[0].id);
    expect(back.board.edges[0].toNode).toBe(back.board.nodes[1].id);
    expect((back.board as { home?: unknown }).home).toEqual({ bg: "cork" });
  });

  it("read a plain canvas too, and refuse what isn't a board", () => {
    const t = readTemplate(JSON.stringify({ nodes: [{ id: "a", type: "text", text: "hi", x: 0, y: 0, width: 100, height: 60 }], edges: [] }));
    expect(t.board.nodes).toHaveLength(1);
    expect(() => readTemplate('{"hello": 1}')).toThrow();
    expect(() => readTemplate("not json")).toThrow();
  });

  it("start with the cards the old start page had", () => {
    const kinds = starterBoard().nodes.map((n) => n.widget);
    expect(kinds).toEqual(expect.arrayContaining(["greeting", "shortcuts", "capture", "tasks", "notes", "daily", "calendar", "sticky"]));
  });

  it("make cards that other apps can still read", () => {
    const n = makeCard("tasks");
    expect(n.type).toBe("text");
    expect(n.text).toContain("Next 7 days");
    expect(n.cfg).toMatchObject({ range: "days", days: 7 });
  });

  it("name days plainly", () => {
    expect(dayLabel("2026-11-01", "2026-11-01")).toBe("Today");
    expect(dayLabel("2026-11-02", "2026-11-01")).toBe("Tomorrow");
    expect(dayLabel("2026-10-31", "2026-11-01")).toBe("Yesterday");
  });
});
