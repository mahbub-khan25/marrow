// The global menu bar: the window runs what's clicked, keeps the menu's labels,
// toggles and usable items up to date, and every item the Rust menu has is
// handled here and translated.
import { describe, expect, it, vi } from "vitest";

const sent: { cmd: string; args: Record<string, unknown> }[] = [];
let handler: ((e: { payload: string }) => void) | null = null;
vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(async (cmd: string, args: Record<string, unknown>) => {
    sent.push({ cmd, args });
    return [
      ["menu-file", "File"],
      ["new-note", "New note"],
      ["sidebar", "Sidebar"],
    ];
  }),
}));
vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(async (_name: string, h: (e: { payload: string }) => void) => {
    handler = h;
    return () => {};
  }),
}));

import { EditorState } from "@codemirror/state";
import { EditorView } from "@codemirror/view";
import { history } from "@codemirror/commands";
import { editCommand, initAppMenu, menuLabels, runMenuAction, syncAppMenu, type MenuState } from "./appmenu";
import { setLanguage, translate } from "./i18n";
import { textSizeKey } from "./util";
import model from "../../crates/marrow-appmenu/src/model.rs?raw";
import main from "./main.ts?raw";

const last = () => sent[sent.length - 1];
const flush = () => new Promise((r) => setTimeout(r, 0));

/** The action names in the Rust menu (`Action::name`). */
const rustActions = [...model.matchAll(/Action::\w+ => "([a-z-]+)",/g)].map((m) => m[1]);
/** The English labels in the Rust menu, without mnemonics. */
const rustLabels = [...model.matchAll(/(?:item|check)\(\w+, "([^"]+)"|Menu\(\s*"menu-\w+",\s*"([^"]+)"/g)].map((m) => (m[1] ?? m[2]).replace(/_/g, ""));

describe("the global menu", () => {
  it("handles every action of the Rust menu", () => {
    expect(rustActions.length).toBeGreaterThan(40);
    const body = main.slice(main.indexOf("function menuActions()"), main.indexOf("function menuState()"));
    // Cut, Copy and Paste are done by WebKit itself (`Action::webview_command`).
    const webkit = ["cut", "copy", "paste"];
    expect(model).toMatch(/Action::Cut => Some\("Cut"\),\s*Action::Copy => Some\("Copy"\),\s*Action::Paste => Some\("Paste"\),\s*_ => None/);
    for (const name of rustActions) {
      const handled = new RegExp(`(^|\\s)("${name}"|${name}): `, "m");
      if (webkit.includes(name)) expect(body, name).not.toMatch(handled);
      else expect(body, name).toMatch(handled);
    }
  });

  it("has every label's Bangla in one place, i18n-bn-menu.ts", async () => {
    const { BN } = await import("./i18n-bn");
    const { BN_MENU } = await import("./i18n-bn-menu");
    expect(rustLabels.length).toBeGreaterThan(45);
    for (const l of rustLabels) expect(BN_MENU[l], l).toBeTruthy();
    // Nothing else in the file, and nowhere else: one word, one place.
    expect(Object.keys(BN_MENU).sort()).toEqual([...rustLabels].sort());
    for (const l of rustLabels) expect(BN[l], `${l} is also in i18n-bn.ts`).toBeUndefined();
    // The rest of the interface uses the same words.
    expect(translate("Close tab")).toBe(BN_MENU["Close tab"]);
  });

  it("knows the text size keys", () => {
    const k = (key: string, code: string) => textSizeKey({ key, code });
    expect(k("=", "Equal")).toBe(1);
    expect(k("+", "Equal")).toBe(1); // Ctrl+Shift+=
    expect(k("+", "NumpadAdd")).toBe(1);
    expect(k("-", "Minus")).toBe(-1);
    expect(k("-", "NumpadSubtract")).toBe(-1);
    expect(k("0", "Digit0")).toBe(0);
    expect(k("০", "Digit0")).toBe(0); // a Bangla layout
    expect(k("k", "KeyK")).toBeNull();
    expect(k("9", "Digit9")).toBeNull();
  });

  it("translates the labels only for Bangla", () => {
    const en: [string, string][] = [
      ["menu-file", "File"],
      ["new-note", "New note"],
      ["x", "Not a phrase Marrow has"],
    ];
    expect(menuLabels(en, "en")).toEqual({});
    expect(menuLabels(en, "bn")).toEqual({ "menu-file": "ফাইল", "new-note": "নতুন নোট" });
  });

  it("runs the clicked item", () => {
    const ran: string[] = [];
    const actions = { "new-note": () => ran.push("new-note") };
    expect(runMenuAction(actions, "new-note")).toBe(true);
    expect(runMenuAction(actions, "nope")).toBe(false);
    expect(ran).toEqual(["new-note"]);
  });

  it("listens for clicks and keeps the menu up to date", async () => {
    let state: MenuState = { checked: { sidebar: true }, enabled: { save: false } };
    const ran: string[] = [];
    await initAppMenu({ sidebar: () => ran.push("sidebar") }, () => state);
    await flush();
    expect(last()).toEqual({ cmd: "app_menu_sync", args: { labels: {}, checked: { sidebar: true }, enabled: { save: false } } });

    handler!({ payload: "sidebar" });
    expect(ran).toEqual(["sidebar"]);

    // Nothing changed: nothing sent.
    const n = sent.length;
    syncAppMenu();
    syncAppMenu();
    await flush();
    expect(sent.length).toBe(n);

    state = { checked: {}, enabled: {} };
    syncAppMenu();
    await flush();
    expect(sent.length).toBe(n + 1);
    expect(last().args.checked).toEqual({});

    // In Bangla the labels go too.
    setLanguage("bn");
    syncAppMenu();
    await flush();
    expect(last().args.labels).toEqual({ "menu-file": "ফাইল", "new-note": "নতুন নোট", sidebar: "সাইডবার" });
    setLanguage("en");
  });
});

describe("Edit menu items", () => {
  function editor(text: string) {
    const parent = document.createElement("div");
    document.body.append(parent);
    const view = new EditorView({ state: EditorState.create({ doc: text, extensions: [history()] }), parent });
    return view;
  }

  it("undo and redo use the editor's own history", () => {
    const view = editor("hello");
    view.dispatch({ changes: { from: 5, insert: " world" }, userEvent: "input" });
    expect(editCommand("undo", () => view)).toBe(true);
    expect(view.state.doc.toString()).toBe("hello");
    expect(editCommand("redo", () => view)).toBe(true);
    expect(view.state.doc.toString()).toBe("hello world");
    view.destroy();
  });

  it("select all selects the whole note", () => {
    const view = editor("one\ntwo");
    expect(editCommand("select-all", () => view)).toBe(true);
    expect(view.state.selection.main.from).toBe(0);
    expect(view.state.selection.main.to).toBe(7);
    view.destroy();
  });

  it("does nothing with no editor and no text box", () => {
    (document.activeElement as HTMLElement | null)?.blur();
    expect(editCommand("find", () => null)).toBe(false);
    expect(editCommand("undo", () => null)).toBe(false);
  });
});
