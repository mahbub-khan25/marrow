// The whole window on the welcome screen (no vault open), driven through the
// global menu: every item it offers there works without a vault, and none
// shows an error. (Dark mode once tried to draw Home and said "Couldn't open
// Home: No vault is open".)
import { describe, expect, it, vi } from "vitest";

const invoked: { cmd: string; args: Record<string, unknown> }[] = [];
/** Once the window is up, any other call to the backend fails as it would with no vault. */
let strict = false;
const listeners = new Map<string, (e: { payload: unknown }) => void>();

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(async (cmd: string, args: Record<string, unknown> = {}) => {
    invoked.push({ cmd, args });
    switch (cmd) {
      case "app_platform":
        return { os: "linux", mobile: false };
      case "last_vault":
        return null;
      case "app_menu_sync":
        return [];
      default:
        if (strict) throw "No vault is open";
        return null;
    }
  }),
  convertFileSrc: (p: string) => p,
  Channel: class {},
}));
vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(async (name: string, h: (e: { payload: unknown }) => void) => {
    listeners.set(name, h);
    return () => {};
  }),
  emit: vi.fn(async () => {}),
}));
vi.mock("@tauri-apps/api/window", () => ({
  getCurrentWindow: () => ({
    onCloseRequested: async () => () => {},
    setTitle: async () => {},
    isVisible: async () => true,
    listen: async () => () => {},
  }),
}));
vi.mock("@tauri-apps/api/path", () => ({ homeDir: async () => "/home/me", join: async (...p: string[]) => p.join("/") }));
vi.mock("@tauri-apps/api/app", () => ({ getVersion: async () => "1.9.0" }));
vi.mock("@tauri-apps/plugin-log", () => ({ error: vi.fn(async () => {}), info: vi.fn(async () => {}), warn: vi.fn(async () => {}) }));
vi.mock("@tauri-apps/plugin-dialog", () => ({ ask: vi.fn(async () => false), open: vi.fn(async () => null), save: vi.fn(async () => null), message: vi.fn(async () => {}) }));
vi.mock("@tauri-apps/plugin-opener", () => ({ revealItemInDir: vi.fn(async () => {}), openUrl: vi.fn(async () => {}), openPath: vi.fn(async () => {}) }));

import html from "../index.html?raw";
import model from "../../crates/marrow-appmenu/src/model.rs?raw";

const errors: string[] = [];
// (Vitest also fails the run on any unhandled rejection.)

const settle = async () => {
  for (let i = 0; i < 5; i++) await new Promise((r) => setTimeout(r, 0));
};

/** The error toast's text, if one is up. */
function errorToast(): string | null {
  const t = document.getElementById("toast");
  return t && !t.hidden && t.dataset.kind === "error" ? t.textContent : null;
}

describe("the global menu on the welcome screen", () => {
  it("offers only what works without a vault, and nothing shows an error", async () => {
    window.matchMedia ??= ((q: string) => ({
      matches: false,
      media: q,
      addEventListener() {},
      removeEventListener() {},
      addListener() {},
      removeListener() {},
      onchange: null,
      dispatchEvent: () => false,
    })) as unknown as typeof window.matchMedia;
    document.body.innerHTML = new DOMParser().parseFromString(html, "text/html").body.innerHTML;
    window.addEventListener("error", (e) => errors.push(`error: ${e.message}`));

    // jsdom measures no text; CodeMirror asks.
    Range.prototype.getClientRects ??= () => [] as unknown as DOMRectList;
    Range.prototype.getBoundingClientRect ??= () => new DOMRect();

    await import("./main");
    await settle();
    expect(errorToast(), `at start, after ${invoked.map((c) => c.cmd).join(", ")}`).toBeNull();
    expect(document.getElementById("welcome")!.hidden).toBe(false);
    expect(document.getElementById("app")!.hidden).toBe(true);

    strict = true;
    const click = listeners.get("app-menu");
    expect(click, "the window listens to the menu on Linux").toBeTruthy();
    const syncs = invoked.filter((c) => c.cmd === "app_menu_sync");
    const enabled = syncs[syncs.length - 1].args.enabled as Record<string, boolean>;

    // What the welcome screen keeps.
    for (const name of ["about", "shortcuts", "help", "dark-mode", "icon-bar", "status-bar", "text-bigger", "text-smaller", "text-default"]) {
      expect(enabled[name], name).not.toBe(false);
    }
    expect(enabled.settings).toBe(false);
    expect(enabled.home).toBe(false);
    expect(enabled["new-note"]).toBe(false);

    // Every item the menu would send here (a disabled one is never sent), except
    // the ones that open a system dialog or quit.
    const names = [...model.matchAll(/Action::\w+ => "([a-z-]+)",/g)].map((m) => m[1]);
    const skip = new Set(["quit", "open-vault", "create-vault", "cut", "copy", "paste"]);
    for (const name of names) {
      if (skip.has(name) || enabled[name] === false) continue;
      invoked.length = 0;
      click!({ payload: name });
      await settle();
      expect(errorToast(), name).toBeNull();
      expect(errors, name).toEqual([]);
      const vaultCalls = invoked.filter((c) => !["app_menu_sync", "app_platform"].includes(c.cmd));
      expect(vaultCalls.map((c) => c.cmd), `${name} asked the backend for vault things`).toEqual([]);
      // Close what it opened.
      document.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true }));
      document.querySelectorAll(".help-wrap, .backdrop.help").forEach((el) => el.remove());
    }

    // Settings opened from About shows only the sections that need no vault.
    click!({ payload: "about" });
    await settle();
    const nav = [...document.querySelectorAll("#settings-nav button")].map((b) => b.textContent?.trim());
    expect(nav).toEqual(["Hotkeys", "About"]);
    expect(document.querySelector("#settings-body h2")!.textContent).toBe("About");
    const keys = document.querySelector("#settings-body")!;
    click!({ payload: "shortcuts" });
    await settle();
    expect(keys.textContent).toContain("Quit Marrow");
    expect(keys.textContent).toContain("Bigger text");
  });
});
