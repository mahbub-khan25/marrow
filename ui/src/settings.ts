// Settings (Ctrl+,): Editor, Appearance, Hotkeys, Vault, AI, Customize.

import { getVersion } from "@tauri-apps/api/app";

import { api, type MirrorStatus, type VaultSettings } from "./api";
import { renderAi } from "./ai-settings";
import { renderCustomize, type CustomizeApp } from "./extras";
import { providerIcon, syncLine } from "./cloud";
import logoUrl from "./assets/logo-mark.svg";
import { prefs, setPref, type Prefs } from "./prefs";
import { $, h, icon, isMac, kbd } from "./util";

export interface SettingsApp {
  vaultName(): string;
  vaultRoot(): string;
  folders(): string[];
  vaultSettings(): VaultSettings;
  setVaultSettings(s: VaultSettings): void;
  /** Daily notes not in the folders the daily layout uses. */
  misplacedDaily(): number;
  organizeDaily(): Promise<void>;
  openAnotherVault(): void;
  /** Import from Notion, Evernote, OneNote… */
  importNotes(): void;
  createVault(): void;
  /** Two-way sync with a cloud folder, if it's on. */
  mirror(): MirrorStatus | null;
  setupSync(): void;
  stopSync(): void;
  revealVault(): void;
  /** CSS snippets and plugins. */
  customize(): CustomizeApp;
}

/** Updates the sync status line, if Settings is showing it. */
export function updateSyncLine(text: string) {
  const el = document.getElementById("sync-line");
  if (el) el.textContent = text;
}

type Tab = "editor" | "appearance" | "hotkeys" | "vault" | "ai" | "customize" | "about";
const TABS: [Tab, string, string][] = [
  ["editor", "Editor", "pencil-simple-line"],
  ["appearance", "Appearance", "text-aa"],
  ["hotkeys", "Hotkeys", "keyboard"],
  ["vault", "Vault", "vault"],
  ["ai", "AI", "sparkle"],
  ["customize", "Customize", "paint-brush"],
  ["about", "About", "info"],
];

let app: SettingsApp;
let tab: Tab = "editor";

export function initSettings(a: SettingsApp) {
  app = a;
  $("settings").addEventListener("mousedown", (e) => {
    if (e.target === $("settings")) closeSettings();
  });
  $("settings").addEventListener("keydown", (e) => {
    if (e.key === "Escape") {
      e.stopPropagation();
      closeSettings();
    }
  });
}

export function settingsOpen() {
  return !$("settings").hidden;
}

export function openSettings(t: Tab = tab) {
  tab = t;
  $("settings").hidden = false;
  document.documentElement.toggleAttribute("data-modal", true);
  render();
  $("settings-body").focus();
}

export function closeSettings() {
  $("settings").hidden = true;
  document.documentElement.toggleAttribute("data-modal", false);
}

function seg<T>(opts: [T, string][], value: T, set: (v: T) => void): HTMLElement {
  return h(
    "div.seg",
    { role: "group" },
    ...opts.map(([v, label]) =>
      h(
        "button",
        {
          "aria-pressed": String(v === value),
          onclick: () => {
            set(v);
            render();
          },
        },
        label,
      ),
    ),
  );
}

const isMacLabel = (mac: string, other: string) => (isMac ? mac : other);

const row = (label: string, desc: string, control: HTMLElement) =>
  h("div.set-row", null, h("div", null, h("div.l", null, label), h("div.d", null, desc)), control);

function prefSeg<K extends keyof Prefs>(k: K, opts: [Prefs[K], string][]) {
  return seg(opts, prefs[k], (v) => setPref(k, v));
}

function render() {
  const nav = $("settings-nav");
  nav.replaceChildren(
    h("div.h", null, "Settings"),
    ...TABS.map(([k, label, ic]) =>
      h(
        "button",
        {
          "aria-selected": String(k === tab),
          onclick: () => {
            tab = k;
            render();
          },
        },
        icon(ic),
        label,
      ),
    ),
  );
  const body = $("settings-body");
  const title = TABS.find((t) => t[0] === tab)![1];
  body.replaceChildren(
    h("button.icon-btn.close", { title: "Close", onclick: closeSettings }, icon("x")),
    h("h2", null, title),
  );
  if (tab === "editor") {
    body.append(
      h(
        "div.set-rows",
        null,
        row(
          "Open notes in",
          `Preview renders Markdown; Source shows the raw text. ${kbd("E")} switches either way.`,
          prefSeg("defaultMode", [
            ["preview", "Preview"],
            ["source", "Source"],
          ]),
        ),
        row(
          "Live preview in Source",
          "Hides Markdown marks like ** and [[ ]] except on the line you're editing.",
          prefSeg("livePreview", [
            [true, "On"],
            [false, "Off"],
          ]),
        ),
        row(
          "Formatting toolbar",
          "Buttons for headings, bold, lists, links and tables while you write. Selecting text also shows a small bar.",
          prefSeg("toolbar", [
            [true, "Show"],
            [false, "Hide"],
          ]),
        ),
        row(
          "Word goal in focus mode",
          "Focus mode (F11) hides everything but the note. With a goal, a bar at the bottom fills as the note grows.",
          prefSeg("wordGoal", [
            [0, "None"],
            [250, "250"],
            [500, "500"],
            [1000, "1000"],
            [2000, "2000"],
          ]),
        ),
        row(
          "Quick capture shortcut",
          "Press it in any app to jot something into today's note, while Marrow is open (even minimised).",
          prefSeg("captureKey", [
            ["", "Off"],
            ["CommandOrControl+Alt+N", isMacLabel("⌘⌥N", "Ctrl+Alt+N")],
            ["CommandOrControl+Alt+Space", isMacLabel("⌘⌥Space", "Ctrl+Alt+Space")],
            ["CommandOrControl+Shift+Space", isMacLabel("⌘⇧Space", "Ctrl+Shift+Space")],
          ]),
        ),
        row(
          "Spell check",
          "Underlines misspelt words while you write in Source. Right-click a word for suggestions. Uses your computer's dictionaries and languages.",
          prefSeg("spellcheck", [
            [true, "On"],
            [false, "Off"],
          ]),
        ),
        row(
          "Bring back old notes",
          "On the start screen: a note you haven't opened in a long time, the same day in earlier years, and a weekly review from Friday to Sunday.",
          prefSeg("resurface", [
            [true, "On"],
            [false, "Off"],
          ]),
        ),
        row(
          "Bangla typing",
          "Type Bangla with an English keyboard, Avro Phonetic style: ami → আমি. F12 or the অ button in the status bar switches it on and off.",
          prefSeg("banglaKeys", [
            [true, "Show the button"],
            [false, "Off"],
          ]),
        ),
        row(
          "Vim keys",
          "Edit in Source with Vim's normal, insert and visual modes (Esc, hjkl, dd, :w …). Off by default.",
          prefSeg("vim", [
            [false, "Off"],
            [true, "On"],
          ]),
        ),
        row(
          "Locked notes",
          "After you enter a locked note's password, lock it again when Marrow has been left alone this long.",
          prefSeg("lockAfter", [
            [5, "5 min"],
            [15, "15 min"],
            [60, "1 hour"],
            [0, "When closed"],
          ]),
        ),
        row(
          "Readable line length",
          "Holds the writing column near a comfortable measure.",
          prefSeg("width", [
            ["narrow", "Narrow"],
            ["normal", "Normal"],
            ["wide", "Wide"],
          ]),
        ),
        row(
          "Text size",
          "Body size for reading and writing.",
          prefSeg("size", [
            [16, "16"],
            [18, "18"],
            [20, "20"],
            [22, "22"],
          ]),
        ),
      ),
    );
  } else if (tab === "appearance") {
    body.append(
      h(
        "div.set-rows",
        null,
        row(
          "Interface language",
          "Menus, buttons and messages. Your notes are never changed.",
          prefSeg("language", [
            ["en", "English"],
            ["bn", "বাংলা"],
          ]),
        ),
        row(
          "Theme",
          "Follow the system, or keep it light or dark.",
          prefSeg("theme", [
            ["system", "System"],
            ["light", "Light"],
            ["dark", "Dark"],
          ]),
        ),
        row(
          "Inspector",
          "Local graph, outline, backlinks and outgoing links beside the note.",
          prefSeg("inspector", [
            [true, "Show"],
            [false, "Hide"],
          ]),
        ),
        row(
          "Links",
          "How links to other notes look when reading.",
          prefSeg("linkStyle", [
            ["underline", "Underline"],
            ["tint", "Tint"],
          ]),
        ),
      ),
    );
  } else if (tab === "hotkeys") {
    const keys: [string, string][] = [
      ["Command palette", kbd("K")],
      ["Open a note", `${kbd("O")}  or  ${kbd("P")}`],
      ["Open a note in a new tab", kbd("T")],
      ["New note", kbd("N")],
      ["Today's daily note", kbd("D")],
      ["Toggle preview / source", kbd("E")],
      ["Graph view", kbd("G")],
      ["Split pane", kbd("\\")],
      ["Search all notes", kbd("F", true)],
      ["Find in this note", kbd("F")],
      ["Save now", kbd("S")],
      ["Close tab", kbd("W")],
      ["Next / previous tab", `${kbd("Tab")}  /  ${kbd("Tab", true)}`],
      ["Settings", kbd(",")],
      ["Focus mode", "F11"],
      ["AI: edit selection or write (source)", kbd("J")],
      ["Follow link (source)", `${kbd("click")}  or  ${kbd("Enter")}`],
      ["Open link in split", `${kbd("click")} in preview`],
    ];
    body.append(
      h(
        "table.table",
        null,
        h("thead", null, h("tr", null, h("th", null, "Action"), h("th", { style: "text-align:right" }, "Shortcut"))),
        h(
          "tbody",
          null,
          ...keys.map(([a, k]) => h("tr", null, h("td", null, a), h("td", { style: "text-align:right;letter-spacing:0.03em;white-space:nowrap" }, k))),
        ),
      ),
    );
  } else if (tab === "about") {
    const ver = h("span", null, "…");
    void getVersion().then((v) => (ver.textContent = v), () => (ver.textContent = "unknown"));
    body.append(
      h(
        "div.set-vault",
        null,
        h("div", { style: "display:flex;align-items:center;gap:18px" }, h("span.logo-mark.logo-lg", null, h("img", { src: logoUrl, alt: "" })), h("div", null, h("div", { style: "font-size:22px;font-weight:600" }, "Marrow"), h("div.d", { style: "color:var(--ink-70)" }, "Version ", ver))),
        h(
          "div.line",
          null,
          "A local-first notes app. Your notes are plain Markdown files in a folder you choose; Marrow reads and writes them in place and keeps its own data in a hidden .marrow folder.",
        ),
        h("div.line", null, "Built in Rust with Tauri and CodeMirror. Canvases use the open JSON Canvas format."),
        h(
          "div.line",
          null,
          "Something went wrong? Marrow keeps a small diagnostic log on this computer. It never contains your notes or passwords, and nothing is sent anywhere. Attach it to a bug report. ",
          h("button.btn", { type: "button", onclick: () => void api.openLogFolder() }, "Show the log"),
        ),
      ),
    );
  } else if (tab === "ai") {
    renderAi(body, render);
  } else if (tab === "customize") {
    void renderCustomize(body, render, app.customize());
  } else {
    renderVault(body);
  }
}

/** "Sync": off, on (with status), or not needed (the vault is already in a cloud folder). */
function syncRow(): HTMLElement {
  const m = app.mirror();
  const desc = h("div.d", { id: m ? "sync-line" : undefined });
  const control = h("div.acts", { style: "margin:0" });
  if (m) {
    desc.textContent = syncLine(m);
    control.append(
      h("button.btn.btn-secondary", { onclick: () => void api.syncNow(Boolean(m.stop)) }, icon("arrows-clockwise"), "Sync now"),
      h(
        "button.btn.btn-secondary",
        {
          onclick: () => {
            app.stopSync();
            window.setTimeout(render, 50);
          },
        },
        "Stop",
      ),
    );
  } else {
    desc.textContent = "Keep this vault the same on all your devices, both ways: through Dropbox, OneDrive, Google Drive or iCloud, or your own Nextcloud or WebDAV server.";
    control.append(
      h(
        "button.btn.btn-secondary",
        {
          onclick: () => {
            closeSettings();
            app.setupSync();
          },
        },
        icon("cloud"),
        "Set up…",
      ),
    );
    // Not needed when the vault already lives in a cloud folder.
    void api.vaultCloud().then((c) => {
      if (!c) return;
      desc.replaceChildren(providerIcon(c.provider), ` This vault is in ${c.label}, so ${c.label} already keeps it on your other devices.`);
      control.remove();
    }, () => {});
  }
  return h("div.set-row", null, h("div", null, h("div.l", null, "Sync"), desc), control);
}

/** Offers to move daily notes into the folders the layout uses, when some aren't. */
function dailyMoveRow(): HTMLElement | null {
  const n = app.misplacedDaily();
  if (!n) return null;
  const btn = h("button.btn.btn-secondary", null, `Move ${n} daily note${n === 1 ? "" : "s"}`) as HTMLButtonElement;
  btn.onclick = async () => {
    btn.disabled = true;
    btn.textContent = "Moving…";
    await app.organizeDaily();
    render();
  };
  return row("Existing daily notes", `${n} daily note${n === 1 ? " is" : "s are"} not organized this way yet. Moving them keeps every link working.`, btn);
}

function renderVault(body: HTMLElement) {
  const vs = app.vaultSettings();
  const save = (patch: Partial<VaultSettings>) => {
    const next = { ...app.vaultSettings(), ...patch };
    app.setVaultSettings(next);
    render();
  };
  const folders = app.folders();
  const list = h("datalist", { id: "folder-list" }, ...folders.map((f) => h("option", { value: f })));
  const folderInput = (value: string, onSet: (v: string) => void, placeholder: string) => {
    const el = h("input.input", { value, list: "folder-list", placeholder, spellcheck: false });
    el.addEventListener("change", () => onSet(el.value.trim().replace(/^\/+|\/+$/g, "")));
    return el;
  };
  const box = h(
    "div.set-vault",
    null,
    list,
    h("div.field", null, h("label", null, "Vault name"), h("input.input", { value: app.vaultName(), readOnly: true })),
    h("div.field", null, h("label", null, "Location"), h("input.input", { value: app.vaultRoot(), readOnly: true, title: app.vaultRoot() })),
    h("div.line", { id: "vault-line" }),
    h(
      "div.set-rows",
      null,
      syncRow(),
      row(
        "New notes go in",
        "Where Ctrl+N and new links put a note.",
        seg(
          [
            ["root", "Vault root"],
            ["current", "Current folder"],
            ["folder", "A folder"],
          ] as [VaultSettings["newNoteLocation"], string][],
          vs.newNoteLocation,
          (v) => save({ newNoteLocation: v }),
        ),
      ),
      vs.newNoteLocation === "folder"
        ? row("Folder for new notes", "Created if it doesn't exist.", folderInput(vs.newNoteFolder, (v) => save({ newNoteFolder: v }), "Inbox"))
        : null,
      row("Daily notes folder", `Where ${kbd("D")} keeps one note per day, named by date.`, folderInput(vs.dailyFolder, (v) => save({ dailyFolder: v }), "Vault root")),
      row(
        "Organize daily notes",
        "Year and month folders keep years of notes easy to browse, e.g. Daily › 2026 › 10 October › 2026-10-24.",
        seg(
          [
            ["year-month", "Year and month"],
            ["flat", "One folder"],
          ] as ["year-month" | "flat", string][],
          vs.dailyLayout ?? "year-month",
          (v) => save({ dailyLayout: v }),
        ),
      ),
      dailyMoveRow(),
      row(
        "Templates folder",
        "Notes here appear under Templates in the / menu and can be used by buttons.",
        folderInput(vs.templateFolder, (v) => save({ templateFolder: v }), "Templates"),
      ),
      row(
        "Link format",
        "How Marrow writes links when it adds or updates them.",
        seg(
          [
            ["shortest", "Shortest"],
            ["relative", "Relative"],
            ["absolute", "Full path"],
          ] as [VaultSettings["linkFormat"], string][],
          vs.linkFormat,
          (v) => save({ linkFormat: v }),
        ),
      ),
    ),
    h(
      "div.acts",
      null,
      h("button.btn.btn-secondary", { onclick: () => app.revealVault() }, icon("folder-open"), "Show in file manager"),
      h(
        "button.btn.btn-secondary",
        {
          onclick: () => {
            closeSettings();
            app.openAnotherVault();
          },
        },
        icon("swap"),
        "Open another folder",
      ),
      h(
        "button.btn.btn-secondary",
        {
          onclick: () => {
            closeSettings();
            app.createVault();
          },
        },
        icon("folder-simple-plus"),
        "Create a vault",
      ),
      h(
        "button.btn.btn-secondary",
        {
          onclick: () => {
            closeSettings();
            app.importNotes();
          },
        },
        icon("tray-arrow-down"),
        "Import notes…",
      ),
    ),
  );
  body.append(box);
  void api.vaultStats().then((s) => {
    const el = document.getElementById("vault-line");
    if (el)
      el.textContent = `${s.notes} ${s.notes === 1 ? "note" : "notes"} and ${s.links} ${s.links === 1 ? "link" : "links"}, stored as plain Markdown files. Nothing leaves this computer unless you sync the folder.`;
  });
}
