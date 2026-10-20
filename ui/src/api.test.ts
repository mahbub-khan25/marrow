// Putting a note into a locked folder asks for the password, then retries.
import { describe, expect, it, vi } from "vitest";

const calls: string[] = [];
let unlocked = false;
vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(async (cmd: string) => {
    calls.push(cmd);
    if (!unlocked) throw "LOCKED_FOLDER:Private";
    return "Private/New.md";
  }),
  convertFileSrc: (p: string) => p,
}));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn() }));

import { api, setUnlockHandler } from "./api";

describe("notes into locked folders", () => {
  it("asks for the folder's password and tries again", async () => {
    setUnlockHandler(async (folder) => {
      expect(folder).toBe("Private");
      unlocked = true;
      return true;
    });
    expect(await api.createFile("Private", "New", "md", "secret")).toBe("Private/New.md");
    expect(calls).toEqual(["create_file", "create_file"]);
  });

  it("says plainly when the password isn't given", async () => {
    unlocked = false;
    setUnlockHandler(async () => false);
    await expect(api.movePath("a.md", "Private")).rejects.toThrow("“Private” is locked");
  });
});
