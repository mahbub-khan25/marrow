import "./style.css";

import { homeDir } from "@tauri-apps/api/path";
import { listen } from "@tauri-apps/api/event";
import { error as logError } from "@tauri-apps/plugin-log";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { ask, open as openDialog } from "@tauri-apps/plugin-dialog";
import { revealItemInDir } from "@tauri-apps/plugin-opener";
import type { EditorState } from "@codemirror/state";

import {
  api,
  displayName,
  isCanvasPath,
  isNotePath,
  onMirrorChanged,
  onVaultChanged,
  parentOf,
  setUnlockHandler,
  type ConflictCopy,
  type EngineEvent,
  type GraphNode,
  type Mention,
  type MirrorStatus,
  type SyncReport,
  type RenameReport,
  type VaultSettings,
} from "./api";
import { aiOnEditor, aiOnNote, aiOnVault } from "./ai-panel";
import { CanvasView } from "./canvas";
import { exportHtml, printNote } from "./export";
import { openHistory } from "./history";
import { openTimeMachine } from "./timemachine";
import { openHelp } from "./help";
import { openSwitch, type SwitchFrom } from "./switch";
import { setLanguage, uiLocale } from "./i18n";
import { markSeen, openWeeklyReview, renderResurface, seenMoved, type ResurfaceApp } from "./resurface";
import { openCapture } from "./capture";
import { usePropertyTypes } from "./properties";
import { maybeAskBackground, TasksView } from "./tasks";
import { lineWithReminder, pickReminder, reminderOf, startReminders, whenLabel } from "./reminders";
import { openImport } from "./import";
import { openPublish } from "./publish";
import { present } from "./present";
import { applySnippets, initPlugins, pluginCommands, startPlugins } from "./extras";
import {
  initLocks,
  isLockedError,
  isLockedNote,
  lockedFolderOf,
  lockFolderFlow,
  lockNoteFlow,
  onLockList,
  refreshLocked,
  removeLockFlow,
  setLockAll,
  stopIdleLock,
  unlockPath,
} from "./lock";
import {
  adoptDoc,
  allDocs,
  Doc,
  findDoc,
  forceSave,
  forget,
  initDocs,
  moved as docsMoved,
  offerBackup,
  onDiskChanged,
  onDiskDeleted,
  openDoc,
  overallState,
  releaseUnused,
  replaceText,
  save,
  saveAll,
  keepDrafts,
} from "./docs";
import { editNote } from "./database";
import { makeState } from "./editor";
import { GraphPanel } from "./graphpanel";
import { Inspector } from "./inspector";
import { setLivePreview } from "./livepreview";
import { applyWriting, onVimWrite } from "./writing";
import { hideMenu } from "./menu";
import { NotePane, type At } from "./notepane";
import type { ButtonSpec } from "./preview";
import { createVaultDialog, setupSyncDialog, syncLine } from "./cloud";
import { closePalette, initPalette, openPalette, paletteOpen, type Command, type OpenHow } from "./palette";
import { applyPrefs, onPrefs, prefs, setPref, type Mode } from "./prefs";
import { closeSettings, initSettings, openSettings, settingsOpen, updateSyncLine } from "./settings";
import { SearchView, TagsView } from "./sidebar";
import { CalendarView } from "./calendar";
import { saveFiles } from "./attach";
import { type DailyLayout, dailyIndex as indexDaily, dailyPathFor, misplaced as misplacedDaily } from "./daily";
import { Tree } from "./tree";
import { $, ago, h, icon, isMac, isoDay, kbd, longDate, storage, store, wordCount } from "./util";

// ------------------------------------------------------------------ state

interface NoteTab {
  kind: "note";
  doc: Doc;
  /** Editor state while in the background (undo history, cursor). */
  state: EditorState | null;
  scrollTop: number;
  mode: Mode;
}
interface GraphTab {
  kind: "graph";
}
interface CanvasTab {
  kind: "canvas";
  path: string;
}
type Tab = NoteTab | GraphTab | CanvasTab;

let vaultRoot = "";
let vaultName = "";
let vaultSettings: VaultSettings = {
  linkFormat: "shortest",
  newNoteLocation: "current",
  newNoteFolder: "",
  dailyFolder: "Daily",
  dailyLayout: "year-month",
  templateFolder: "Templates",
  starred: [],
  snippets: [],
  plugins: [],
};
let tabs: Tab[] = [];
let active = -1;
let split: { doc: Doc; mode: Mode } | null = null;
let side = "files" as "files" | "search" | "tags" | "tasks" | "calendar";
let sideOpen = true;
let recent: string[] = [];
let stats = { notes: 0, links: 0 };

let primary: NotePane;
let splitPane: NotePane;
let tree: Tree;
let searchView: SearchView;
let tagsView: TagsView;
/** The device: phones and tablets get touch-sized controls and no desktop-only actions. */
let platform: { os: string; mobile: boolean } = { os: "", mobile: false };
let calendarView: CalendarView;
let tasksView: TasksView;
let inspector: Inspector;
let graphPanel: GraphPanel | null = null;
let canvasView: CanvasView;

const activeTab = (): Tab | undefined => tabs[active];
const activeNote = (): NoteTab | null => {
  const t = activeTab();
  return t?.kind === "note" ? t : null;
};

// ------------------------------------------------------------- status bar

function updateSaveState() {
  const s = overallState();
  const el = $("save-state");
  el.dataset.state = s;
  el.querySelector(".label")!.textContent = { saved: "Saved", unsaved: "Edited", saving: "Saving…", error: "Not saved" }[s];
  // Dirty dots on tabs.
  for (const el of document.querySelectorAll<HTMLElement>(".tab[data-i]")) {
    const t = tabs[Number(el.dataset.i)];
    el.querySelector(".dirty")?.toggleAttribute("hidden", !(t?.kind === "note" && t.doc.dirty));
  }
}

// ------------------------------------------------------------ focus mode

let focusOn = false;

/** Hides everything but the note; F11, the palette or the Exit button switch it. */
function toggleFocus(on = !focusOn) {
  if (on && !activeNote()) return toast("Open a note first.");
  focusOn = on;
  document.documentElement.toggleAttribute("data-focus", on);
  $("focus-bar").hidden = !on;
  updateStatus();
  if (on) {
    const pane = focusedPane();
    if (pane.mode === "source") pane.view.focus();
  }
}

function renderFocusBar(words: number) {
  const goal = prefs.wordGoal;
  const bar = $("focus-bar");
  const fill = goal ? Math.min(1, words / goal) : 0;
  bar.replaceChildren();
  bar.append(
    h("span.focus-words", null, goal ? `${words} / ${goal} words` : `${words} ${words === 1 ? "word" : "words"}`),
    ...(goal ? [h("span.focus-meter", { "aria-hidden": "true" }, h("span", { style: `width:${(fill * 100).toFixed(1)}%` }))] : []),
    ...(goal && words >= goal ? [h("span.focus-done", null, icon("check-circle"), "Goal reached")] : []),
    h("button.focus-exit", { type: "button", title: "Leave focus mode (F11)", onclick: () => toggleFocus(false) }, "Exit"),
  );
}

function updateStatus() {
  const t = activeNote();
  if (focusOn) {
    if (!t) toggleFocus(false);
    else renderFocusBar(wordCount(t.doc.text));
  }
  if (t) {
    const text = t.doc.text;
    const back = primary.info?.backlinks ?? 0;
    const words = wordCount(text);
    $("status").textContent = `${back} ${back === 1 ? "backlink" : "backlinks"} · ${words} ${words === 1 ? "word" : "words"} · ${text.length} characters`;
  } else {
    $("status").textContent = `${stats.notes} ${stats.notes === 1 ? "note" : "notes"}`;
  }
}

async function refreshStats() {
  stats = await api.vaultStats().catch(() => stats);
  const line = `${stats.notes} ${stats.notes === 1 ? "note" : "notes"} · ${stats.links} ${stats.links === 1 ? "link" : "links"}`;
  $("vault-meta").textContent = line;
  $("start-kicker").textContent = `${line} in ${vaultName}`;
  $("start-guide").hidden = stats.notes >= 15;
  updateStatus();
}

// ---------------------------------------------------------------- banners

function showBanner(text: string, kind: "warn" | "error", actions: { label: string; run: () => void }[] = []) {
  const b = $("banner");
  b.replaceChildren(
    h("span", null, text),
    ...actions.map((a) =>
      h(
        "button.btn.btn-secondary",
        {
          onclick: () => {
            hideBanner();
            a.run();
          },
        },
        a.label,
      ),
    ),
    h("button.banner-close", { "aria-label": "Dismiss", onclick: hideBanner }, "✕"),
  );
  b.dataset.kind = kind;
  delete b.dataset.sync;
  b.hidden = false;
}

function hideBanner() {
  const b = $("banner");
  b.hidden = true;
  delete b.dataset.kind;
  delete b.dataset.sync;
}

let toastTimer: number | undefined;
function toast(text: string, kind: "info" | "error" = "info") {
  const el = $("toast");
  el.textContent = text;
  el.dataset.kind = kind;
  el.hidden = false;
  window.clearTimeout(toastTimer);
  toastTimer = window.setTimeout(() => (el.hidden = true), kind === "error" ? 6000 : 3200);
}

// ------------------------------------------------------------------ stage

/** Shows whatever the active tab is, and everything that depends on it. */
function showStage() {
  const t = activeTab();
  const view = !t ? "start" : t.kind;
  $("start").hidden = view !== "start";
  $("panes").hidden = view !== "note";
  $("graph-view").hidden = view !== "graph";
  $("canvas-view").hidden = view !== "canvas";
  const showSplit = view === "note" && split !== null;
  $("pane-split").hidden = !showSplit;
  $("panes").classList.toggle("is-split", showSplit);
  $("split-btn").setAttribute("aria-pressed", String(showSplit));
  // Where the inspector would cover the note (phones, tablets held upright), it opens only when asked.
  const inspOn = inspectorCovers() ? inspectorOnPhone : prefs.inspector;
  const insp = inspOn && view === "note";
  $("app").classList.toggle("no-inspector", !insp);
  $("inspector-btn").setAttribute("aria-pressed", String(inspOn));
  $("inspector").classList.toggle("floating", innerWidth < 1200);
  // Rail
  const daily = t?.kind === "note" && dailyNav(t.doc.path) !== null && displayName(t.doc.path) === isoDay();
  $("rail-today").setAttribute("aria-pressed", String(daily));
  // The calendar follows the daily note that's open.
  const shownDay = t?.kind === "note" && dailyNav(t.doc.path) ? displayName(t.doc.path) : null;
  if (shownDay && side === "calendar" && sideOpen && shownDay !== lastCalendarDay) calendarView.reveal(shownDay);
  if (shownDay) lastCalendarDay = shownDay;
  $("rail-graph").setAttribute("aria-pressed", String(view === "graph"));
  $("rail-canvas").setAttribute("aria-pressed", String(view === "canvas"));
  if (view === "start") void renderStart();
  const title = t?.kind === "note" ? t.doc.name : t?.kind === "canvas" ? displayName(t.path) : t?.kind === "graph" ? "Graph" : "";
  if (!platform.mobile) void getCurrentWindow().setTitle(title ? `${title} — Marrow` : vaultName ? `${vaultName} — Marrow` : "Marrow").catch(() => {});
  tree?.markActive();
  updateStatus();
  updateSaveState();
}

const resurfaceApp: ResurfaceApp = {
  vault: () => vaultRoot,
  dailyIndex: () => dailyIndex(),
  dailyFolder: () => vaultSettings.dailyFolder,
  templateFolder: () => vaultSettings.templateFolder,
  open: (p) => void openPath(p, "replace"),
  create: (folder, name, text) => api.createFile(folder, name, "md", text),
  toast: (t, k) => toast(t, k),
};

function startHeading() {
  const now = new Date();
  const hr = now.getHours();
  $("start-greeting").textContent = hr < 5 ? "Good evening" : hr < 12 ? "Good morning" : hr < 18 ? "Good afternoon" : "Good evening";
  $("start-date").textContent = now.toLocaleDateString(uiLocale(), { weekday: "long", day: "numeric", month: "long" });
}

async function renderStart() {
  startHeading();
  // A new, nearly empty vault: offer to bring notes over.
  const sw = $("switch-from");
  const stats = await api.vaultStats().catch(() => null);
  sw.replaceChildren(
    ...(stats && stats.notes < 15
      ? [
          h(
            "div.rs-card.rs-switch",
            null,
            h("div.rs-k", null, icon("signpost"), "Coming from another app?"),
            h("p.rs-t", null, "Bring all your notes over, with their links, pictures and databases. Marrow shows you each step."),
            h(
              "div.rs-acts",
              null,
              h("button.btn.btn-secondary", { onclick: () => moveFrom("notion") }, icon("notion-logo"), "From Notion"),
              h("button.btn.btn-secondary", { onclick: () => moveFrom("evernote") }, icon("notebook"), "From Evernote"),
              h("button.btn.btn-ghost", { onclick: () => importNotes() }, "Other apps…"),
            ),
          ),
        ]
      : []),
  );
  const rs = $("resurface");
  if (prefs.resurface) void renderResurface(rs, resurfaceApp).catch(() => rs.replaceChildren());
  else rs.replaceChildren();
  const files = await api.recentFiles(6).catch(() => []);
  const box = $("recent");
  if (files.length === 0) {
    box.replaceChildren();
    return;
  }
  box.replaceChildren(
    h("div.smallcaps", null, "Recently edited"),
    ...files.map((f) =>
      h(
        "button",
        { onclick: (e: MouseEvent) => openPath(f.path, e.ctrlKey || e.metaKey ? "tab" : "replace"), title: f.path },
        h("span.t", null, displayName(f.path)),
        h("span.a", null, ago(f.modifiedMs)),
      ),
    ),
  );
}

// ------------------------------------------------------------------- tabs

function stashActive() {
  const t = activeTab();
  if (t?.kind === "note" && primary.doc === t.doc) {
    const s = primary.stash();
    t.state = s.state;
    t.scrollTop = s.scrollTop;
    t.mode = primary.mode;
  }
}

async function activateTab(i: number, opts: { at?: At; focusTitle?: boolean } = {}) {
  const t = tabs[i];
  if (!t) return;
  if (i !== active) stashActive();
  active = i;
  hideBanner();
  showStage();
  if (t.kind === "note") {
    primary.show(t.doc, { state: t.state, mode: t.mode, scrollTop: t.scrollTop, at: opts.at, focusTitle: opts.focusTitle });
    t.state = null; // the pane owns it now
    remember(t.doc.path);
    tree.reveal(t.doc.path);
    void inspector.show(t.doc.path, t.doc.text);
    void offerBackup(t.doc);
    // It may have changed on disk while it was in the background.
    void api.readNote(t.doc.path).then(
      (disk) => {
        if (disk !== t.doc.saved) void onDiskChanged(t.doc);
      },
      () => {
        if (findDoc(t.doc.path) === t.doc && !t.doc.dirty) onDiskDeleted(t.doc);
      },
    );
  } else if (t.kind === "canvas") {
    remember(t.path);
    tree.reveal(t.path);
    const opened = await canvasView.load(t.path).then(
      () => true,
      (e) => {
        toast(`Couldn't open the canvas: ${e}`, "error");
        return false;
      },
    );
    // Still showing the canvas that couldn't be saved: keep its tab active.
    if (!opened && canvasView.path && canvasView.path !== t.path) {
      const back = tabs.findIndex((x) => x.kind === "canvas" && x.path === canvasView.path);
      if (back >= 0) active = back;
    }
    $("canvas-view").focus({ preventScroll: true });
  } else {
    ensureGraph();
    void refreshGraph();
  }
  renderTabs();
  persistSession();
}

function tabLabel(t: Tab): { label: string; icon: string; title: string } {
  if (t.kind === "graph") return { label: "Graph", icon: "graph", title: "Graph view" };
  if (t.kind === "canvas") return { label: displayName(t.path), icon: "squares-four", title: t.path };
  const daily = dailyNav(t.doc.path) !== null;
  return { label: t.doc.name, icon: daily ? "calendar-blank" : "file-text", title: t.doc.path };
}

function renderTabs() {
  const bar = $("tabs");
  bar.replaceChildren(
    ...tabs.map((t, i) => {
      const { label, icon: ic, title } = tabLabel(t);
      const el = h(
        "div.tab",
        { role: "tab", "aria-selected": String(i === active), title, dataset: { i: String(i) } },
        h("i", { class: `ph-duotone ph-${ic}` }),
        h("span.label", null, label),
        t.kind === "note" ? h("span.dirty", { hidden: !t.doc.dirty }) : null,
        h(
          "button.tab-close",
          {
            title: "Close",
            "aria-label": `Close ${label}`,
            onclick: (e: MouseEvent) => {
              e.stopPropagation();
              void closeTab(i);
            },
          },
          h("i", { class: "ph-duotone ph-x" }),
        ),
      );
      el.addEventListener("click", () => void activateTab(i));
      el.addEventListener("auxclick", (e) => {
        if (e.button === 1) void closeTab(i);
      });
      return el;
    }),
    h("button.tab-new", { title: `New note (${kbd("N")})`, onclick: () => void newNote() }, h("i", { class: "ph-duotone ph-plus" })),
  );
  bar.querySelector('[aria-selected="true"]')?.scrollIntoView({ block: "nearest", inline: "nearest" });
}

/**
 * Saves everything before leaving it behind (closing the window, switching
 * vaults, closing tabs). True when it's safe to go on: all saved, or the
 * person chose to go on with the unsaved text kept as a draft.
 */
async function settleEdits(action: string, only?: Doc[]): Promise<boolean> {
  stashActive();
  const failed = (await saveAll()).filter((d) => !only || only.includes(d));
  const canvasOk = only ? true : await canvasView.flush();
  if (!failed.length && canvasOk) return true;
  const notKept: string[] = (await keepDrafts(failed)).map((d) => `“${d.name}”`);
  if (!canvasOk && canvasView.changesAtRisk) notKept.push("the open canvas");
  const names = [...failed.map((d) => `“${d.name}”`), ...(canvasOk ? [] : ["the open canvas"])];
  const list = names.length > 3 ? `${names.slice(0, 3).join(", ")} and ${names.length - 3} more` : names.join(", ");
  const safe = notKept.length
    ? `Copy your text somewhere safe first: Marrow couldn't keep a copy of ${notKept.join(", ")} either.`
    : "Your text is kept, and Marrow offers it back when you open it again.";
  return ask(`${list} couldn't be saved.\n\n${safe}`, {
    title: "Not saved",
    kind: "warning",
    okLabel: `${action} anyway`,
    cancelLabel: "Stay here",
  });
}

async function closeTab(i: number) {
  const t = tabs[i];
  if (!t) return;
  if (i === active) stashActive();
  if (t.kind === "note" && !(await save(t.doc)) && !(await settleEdits("Close", [t.doc]))) return;
  if (t.kind === "canvas" && canvasView.path === t.path && !(await canvasView.flush()) && !(await settleEdits("Close"))) return;
  tabs.splice(i, 1);
  if (i < active) active--;
  else if (i === active) {
    active = -1;
    if (tabs.length) await activateTab(Math.min(i, tabs.length - 1));
  }
  if (active < 0) {
    primary.clear();
    inspector.clear();
    showStage();
  }
  renderTabs();
  persistSession();
  void releaseUnused(docsInUse());
}

async function closeAllTabs() {
  if (!(await settleEdits("Close the tabs"))) return;
  tabs = [];
  active = -1;
  split = null;
  splitPane.clear();
  primary.clear();
  inspector.clear();
  showStage();
  renderTabs();
  persistSession();
  void releaseUnused(docsInUse());
}

function docsInUse(): Set<Doc> {
  const s = new Set<Doc>();
  for (const t of tabs) if (t.kind === "note") s.add(t.doc);
  if (split) s.add(split.doc);
  return s;
}

function remember(path: string) {
  markSeen(vaultRoot, path);
  recent = [path, ...recent.filter((p) => p !== path)].slice(0, 30);
  store(`marrow.recent.${vaultRoot}`, recent);
}

// ------------------------------------------------------------------ opening

async function openPath(path: string, how: OpenHow = "replace", at?: At): Promise<void> {
  if (isCanvasPath(path)) return openCanvas(path, how === "replace" ? "replace" : "tab");
  if (!isNotePath(path)) {
    toast("Only notes and canvases open in Marrow.");
    return;
  }
  return openNote(path, how, at);
}

/**
 * Opens a note. If a tab already shows it, switches there. `replace` swaps
 * the active note tab (or opens a tab if the active one isn't a note).
 */
let inspectorOnPhone = false;
/** Phones, and tablets too narrow for the inspector beside the note. */
const inspectorCovers = () => isPhone() || innerWidth < 1200;
function toggleInspector() {
  if (inspectorCovers()) {
    inspectorOnPhone = !inspectorOnPhone;
    showStage();
  } else setPref("inspector", !prefs.inspector);
}

/** On a phone the sidebar covers the screen, so it steps aside once something opens. */
const isPhone = () => matchMedia("(max-width: 699px)").matches;
function sideAsideOnPhone() {
  if (isPhone() && sideOpen) setSide(side, false);
  inspectorOnPhone = false;
}

async function openNote(path: string, how: OpenHow = "replace", at?: At, opts: { mode?: Mode; focusTitle?: boolean } = {}): Promise<void> {
  sideAsideOnPhone();
  if (how === "split") return openInSplit(path, at);
  const existing = tabs.findIndex((t) => t.kind === "note" && t.doc.path === path);
  if (existing >= 0) {
    if (existing === active) {
      if (at) primary.reveal(at);
      if (opts.focusTitle) primary.focusTitle();
      return;
    }
    await activateTab(existing, { at, focusTitle: opts.focusTitle });
    return;
  }
  const doc = await openDocAsking(path);
  if (!doc) return;
  stashActive();
  const tab: NoteTab = { kind: "note", doc, state: null, scrollTop: 0, mode: opts.mode ?? prefs.defaultMode };
  if (how === "replace" && activeTab()?.kind === "note") {
    const old = tabs[active] as NoteTab;
    tabs[active] = tab;
    void save(old.doc);
  } else {
    tabs.splice(active + 1, 0, tab);
    active = active + 1;
  }
  const i = tabs.indexOf(tab);
  active = -1; // force a full show
  await activateTab(i, { at, focusTitle: opts.focusTitle });
  void releaseUnused(docsInUse());
}

/** Opens a note's doc, asking for the password first if it's locked. */
async function openDocAsking(path: string): Promise<Doc | null> {
  for (let attempt = 0; ; attempt++) {
    try {
      return await openDoc(path);
    } catch (e) {
      if (isLockedError(e) && attempt === 0) {
        if (await unlockPath(path)) continue;
        return null;
      }
      toast(`Couldn't open ${path}: ${e}`, "error");
      return null;
    }
  }
}

// ------------------------------------------------------------ locked notes

/** Locks a note (or folder) with a password, from the tree or the palette. */
async function lockPath(path: string, isFolder: boolean) {
  const prepare = async () => {
    await saveAll();
  };
  try {
    if (isFolder) {
      const n = await lockFolderFlow(path, prepare);
      if (n !== null) toast(n === 1 ? "Locked the folder and its note." : `Locked the folder and its ${n} notes.`);
    } else if (await lockNoteFlow(path, prepare)) {
      toast(`Locked “${displayName(path)}”.`);
    }
  } catch (e) {
    toast(`Couldn't lock it: ${e}`, "error");
  }
  void refreshTree();
}

async function unlockRemove(path: string, isFolder: boolean) {
  try {
    await saveAll();
    const n = await removeLockFlow(path, isFolder);
    if (n !== null) toast(isFolder ? `Removed the folder's lock (${n} ${n === 1 ? "note" : "notes"}).` : `“${displayName(path)}” is no longer locked.`);
  } catch (e) {
    toast(`Couldn't remove the lock: ${e}`, "error");
  }
  void refreshTree();
}

/**
 * Forgets every password: saves first, then closes notes that need one.
 * (Run by the palette and after the idle time in Settings.)
 */
async function lockAllNow(quiet = false) {
  stashActive();
  // A note whose save failed stays open (its unsaved text would be lost by
  // closing it); its banner asks for the password to save it later.
  const unsaved = new Set(await saveAll());
  await api.lockNow();
  stopIdleLock();
  await refreshLocked();
  const locked = (p: string) => isLockedNote(p) || lockedFolderOf(p) !== null;
  let closed = 0;
  if (split && locked(split.doc.path)) {
    closeSplit();
    closed++;
  }
  for (let i = tabs.length - 1; i >= 0; i--) {
    const t = tabs[i];
    if (t.kind === "note" && locked(t.doc.path) && !unsaved.has(t.doc)) {
      await closeTab(i);
      closed++;
    }
  }
  for (const d of allDocs()) if (locked(d.path) && !unsaved.has(d)) forget(d.path);
  const kept = [...unsaved].filter((d) => locked(d.path));
  if (kept.length)
    toast(`${kept.map((d) => `“${d.name}”`).join(", ")} couldn't be saved, so ${kept.length === 1 ? "it stays" : "they stay"} open. Save, then lock again.`, "error");
  if (!quiet || closed) toast(closed ? "Locked. Notes that need a password were closed." : "Locked. Passwords are needed again.");
}

/** Shows the open note as slides, full screen. */
function presentNote() {
  const pane = focusedPane();
  const d = pane.doc;
  if (!d) return toast("Open a note first.");
  void present(displayName(d.path), d.text, (el) => pane.hydrateFor(el));
}

/** Publish a folder ("" = the whole vault) as a website. */
function publishSite(folder: string) {
  void saveAll().then(() => openPublish({ folder, files: tree.files(), vaultRoot, vaultName, toast }));
}

async function openInSplit(path: string, at?: At): Promise<void> {
  if (!isNotePath(path)) return openPath(path, "tab", at);
  const doc = await openDocAsking(path);
  if (!doc) return;
  const mode = split?.doc === doc ? split.mode : prefs.defaultMode;
  split = { doc, mode };
  if (!activeNote()) {
    // The split sits beside a note; with none open, open this one instead.
    split = null;
    return openNote(path, "tab", at);
  }
  showStage();
  splitPane.show(doc, { mode, at });
  persistSession();
  void releaseUnused(docsInUse());
}

function closeSplit() {
  split = null;
  splitPane.clear();
  showStage();
  persistSession();
  primary.focus();
  void releaseUnused(docsInUse());
}

/** Ctrl+\: shows the first note this one links to beside it (or itself). */
async function toggleSplit() {
  if (split) return closeSplit();
  const t = activeNote();
  if (!t) return;
  const links = await api.noteLinks(t.doc.path).catch(() => ({ outgoing: [] as string[], unresolved: [] }));
  const other = links.outgoing.find((p) => isNotePath(p) && p !== t.doc.path);
  await openInSplit(other ?? t.doc.path);
}

async function openCanvas(path: string, how: "replace" | "tab" = "tab") {
  sideAsideOnPhone();
  const existing = tabs.findIndex((t) => t.kind === "canvas" && t.path === path);
  if (existing >= 0) return activateTab(existing);
  stashActive();
  const tab: CanvasTab = { kind: "canvas", path };
  if (how === "replace" && activeTab()?.kind === "canvas") tabs[active] = tab;
  else {
    tabs.splice(active + 1, 0, tab);
    active++;
  }
  const i = tabs.indexOf(tab);
  active = -1;
  await activateTab(i);
}

function openGraph() {
  const i = tabs.findIndex((t) => t.kind === "graph");
  if (i >= 0) return activateTab(i);
  stashActive();
  tabs.splice(active + 1, 0, { kind: "graph" });
  const at = active + 1;
  active = -1;
  return activateTab(at);
}

/** The rail's canvas button: the last canvas used, or a new one. */
async function openSomeCanvas() {
  const i = tabs.findIndex((t) => t.kind === "canvas");
  if (i >= 0) return activateTab(i);
  const known = new Set(tree.files().filter(isCanvasPath));
  const last = recent.find((p) => known.has(p)) ?? [...known][0];
  if (last) return openCanvas(last);
  return newCanvas();
}

// ---------------------------------------------------------------- creating

/** The folder new notes go in, by the vault's setting. */
function newNoteFolder(): string {
  const folders = new Set(tree.folders());
  switch (vaultSettings.newNoteLocation) {
    case "root":
      return "";
    case "folder":
      return vaultSettings.newNoteFolder;
    default: {
      const t = activeTab();
      const p = t?.kind === "note" ? t.doc.path : t?.kind === "canvas" ? t.path : null;
      const f = p ? parentOf(p) : "";
      return f === "" || folders.has(f) ? f : "";
    }
  }
}

async function newNote(folder = newNoteFolder()) {
  try {
    const path = await api.createNote(folder);
    await refreshTree();
    await openNote(path, "tab", undefined, { mode: "source", focusTitle: true });
  } catch (e) {
    toast(`Couldn't create a note: ${e}`, "error");
  }
}

async function createNamed(name: string, how: OpenHow) {
  try {
    const folder = newNoteFolder();
    const clean = name.replace(/[\\/:*?"<>|#^[\]]/g, " ").replace(/\s+/g, " ").trim() || "Untitled";
    const path = await api.createNote(folder, clean);
    await refreshTree();
    await openNote(path, how === "split" ? "split" : how, undefined, { mode: "source" });
  } catch (e) {
    toast(`Couldn't create “${name}”: ${e}`, "error");
  }
}

async function newFolder(parent = "") {
  try {
    const path = await api.createFolder(parent);
    await refreshTree();
    tree.reveal(path + "/x");
    tree.renameByPath(path);
  } catch (e) {
    toast(`Couldn't create a folder: ${e}`, "error");
  }
}

async function newCanvas(folder = newNoteFolder()) {
  try {
    const path = await api.createFile(folder, "Untitled canvas", "canvas", JSON.stringify({ nodes: [], edges: [] }, null, "\t"));
    await refreshTree();
    await openCanvas(path, "tab");
  } catch (e) {
    toast(`Couldn't create a canvas: ${e}`, "error");
  }
}

/** A note with a database view of `folder`, opened in Preview. */
async function newDatabase(folder: string) {
  const name = `${folder.split("/").pop() || vaultName} database`;
  try {
    const path = await api.createFile(folder, name, "md", "```database\nview: table\n```\n");
    await refreshTree();
    await openNote(path, "tab", undefined, { mode: "preview" });
  } catch (e) {
    toast(`Couldn't create the database: ${e}`, "error");
  }
}

// ------------------------------------------------------------------ daily

const dailyLayout = (): DailyLayout => vaultSettings.dailyLayout ?? "year-month";
let lastCalendarDay: string | null = null;

function dailyPath(day: string) {
  return dailyPathFor(vaultSettings.dailyFolder, day, dailyLayout());
}

/** Daily notes by day, wherever they are in the daily folder. */
function dailyIndex(): Map<string, string> {
  return indexDaily(tree.files(), vaultSettings.dailyFolder);
}

function dailyNav(path: string) {
  const name = displayName(path);
  const all = dailyIndex();
  if (all.get(name) !== path) return null;
  const days = [...all.keys()];
  const i = days.indexOf(name);
  return { label: longDate(name), prev: i > 0 ? all.get(days[i - 1])! : null, next: i < days.length - 1 ? all.get(days[i + 1])! : null };
}

/** Daily notes that aren't in the folders the layout uses. */
function misplacedDailyNotes() {
  return misplacedDaily(tree.files(), vaultSettings.dailyFolder, dailyLayout());
}

/** Moves daily notes into their year/month folders (or back into one folder). */
async function organizeDaily() {
  const moves = misplacedDailyNotes();
  if (!moves.length) return;
  // Open notes are saved first, so nothing typed is left behind in the old place.
  if (!(await settleEdits("Organize daily notes"))) return;
  let done = 0;
  const failed: string[] = [];
  for (const [path, folder] of moves) {
    try {
      await api.movePath(path, folder);
      done++;
    } catch (e) {
      failed.push(`${displayName(path)} (${e})`);
    }
    if (done % 25 === 0) toast(`Organizing daily notes… ${done} of ${moves.length}`);
  }
  await refreshTree();
  if (failed.length) toast(`Moved ${done} daily notes. These couldn't be moved: ${failed.slice(0, 3).join(", ")}${failed.length > 3 ? ` and ${failed.length - 3} more` : ""}.`, "error");
  else toast(dailyLayout() === "year-month" ? `Organized ${done} daily notes into year and month folders.` : `Moved ${done} daily notes into one folder.`);
}

/** Today's daily note, created if needed. Returns its path. */
async function ensureToday(): Promise<string> {
  const day = isoDay();
  const existing = dailyIndex().get(day);
  if (existing) return existing;
  const path = dailyPath(day);
  const made = await api.createFile(parentOf(path), day, "md", "## Log\n- \n\n## Tasks\n- [ ] \n");
  await refreshTree();
  return made;
}

/**
 * Adds a line to today's note, under its "## Log" or "## Tasks" heading when
 * it has one (replacing an empty "- " or "- [ ] " placeholder), or at the end.
 */
async function appendToToday(line: string, section: "Log" | "Tasks") {
  try {
    const path = await ensureToday();
    await editNote(path, (text) => {
      const lines = text.replace(/\s*$/, "").split("\n");
      const head = lines.findIndex((l) => new RegExp(`^#{1,6}\\s+${section}\\s*$`, "i").test(l));
      if (head < 0) return [...lines, "", line].join("\n") + "\n";
      let end = head + 1;
      while (end < lines.length && !/^#{1,6}\s/.test(lines[end])) end++;
      // An empty placeholder item takes the line.
      const empty = lines.slice(head + 1, end).findIndex((l) => /^\s*[-*+]\s*(\[ \])?\s*$/.test(l));
      if (empty >= 0) lines[head + 1 + empty] = line;
      else {
        let at = end;
        while (at > head + 1 && !lines[at - 1].trim()) at--;
        lines.splice(at, 0, line);
      }
      return lines.join("\n") + "\n";
    });
    toast(section === "Tasks" ? "Added to today's tasks." : "Added to today's note.");
  } catch (e) {
    toast(`Couldn't add it to today's note: ${e}`, "error");
    throw e;
  }
}

/** The quick-capture box; `wasHidden` sends Marrow back to the background after. */
function quickCapture(wasHidden = false) {
  openCapture({
    save: async (text, asTask, files) => {
      const time = new Date().toLocaleTimeString(undefined, { hour: "2-digit", minute: "2-digit" });
      const label = text || (files.every((f) => f.type.startsWith("audio")) ? "Voice note" : files.every((f) => f.type.startsWith("image")) ? "Photo" : "Attachment");
      const one = label.replace(/\s*\n\s*/g, " ");
      // Recordings and photos sit under the line, in the same list item.
      const embeds = files.length ? "\n" + (await saveFiles(files, await ensureToday(), false, "\n")).split("\n").map((m) => `  ${m}`).join("\n") : "";
      await appendToToday((asTask ? `- [ ] ${one}` : `- ${time} ${one}`) + embeds, asTask ? "Tasks" : "Log");
      if (side === "tasks" && sideOpen) void tasksView.refresh();
    },
    transcriber: () => api.aiTranscriber(),
    transcribe: async (audio) => api.aiTranscribe(new Uint8Array(await audio.arrayBuffer()), audio.name),
    done: () => {
      if (wasHidden && !platform.mobile) void getCurrentWindow().minimize();
    },
  });
}

let captureKey: string | null = null;
function applyCaptureKey() {
  const want = prefs.captureKey || null;
  if (want === captureKey) return;
  captureKey = want;
  void api.setCaptureShortcut(want).catch((e) => toast(String(e), "error"));
}

async function openToday(how: OpenHow = "replace") {
  return openDay(isoDay(), how);
}

/** Opens a day's daily note, creating it if it doesn't exist yet. */
async function openDay(day: string, how: OpenHow = "replace") {
  const existing = dailyIndex().get(day);
  if (existing) return openNote(existing, how);
  const path = dailyPath(day);
  try {
    const made = await api.createFile(parentOf(path), day, "md", "## Log\n- \n\n## Tasks\n- [ ] \n");
    await refreshTree();
    await openNote(made, how, undefined, { mode: "source" });
  } catch (e) {
    toast(`Couldn't create the note for ${longDate(day)}: ${e}`, "error");
  }
}

// -------------------------------------------------------------- templates

const cleanFolder = (f: string) => f.replace(/^\/+|\/+$/g, "");

/** Notes in the templates folder (and its subfolders). */
function templates(): { name: string; path: string }[] {
  const f = cleanFolder(vaultSettings.templateFolder);
  if (!f) return [];
  return tree
    .files()
    .filter((p) => isNotePath(p) && p.startsWith(f + "/"))
    .sort()
    .map((p) => ({ name: displayName(p), path: p }));
}

/** A template by path or name, with {{title}}, {{date}} and {{time}} filled in. */
async function templateText(ref: string, title: string): Promise<string> {
  const f = cleanFolder(vaultSettings.templateFolder);
  const r = ref.trim();
  const candidates = [r, `${r}.md`, `${f}/${r}`, `${f}/${r}.md`];
  let path = candidates.find((c) => tree.find(c)) ?? null;
  path ??= await api.resolveLink(r, "").catch(() => null);
  if (!path) {
    toast(`Template “${ref}” wasn't found. Put templates in the “${f || "Templates"}” folder.`, "error");
    return "";
  }
  const text = await api.readNote(path).catch(() => "");
  const now = new Date();
  return text
    .replace(/\{\{\s*title\s*\}\}/gi, title)
    .replace(/\{\{\s*date\s*\}\}/gi, isoDay(now))
    .replace(/\{\{\s*time\s*\}\}/gi, now.toLocaleTimeString(undefined, { hour: "2-digit", minute: "2-digit" }));
}

/** ```button blocks that create a note or add to today's note. */
async function runButton(spec: ButtonSpec) {
  const bare = spec.name.replace(/^(add|new|create)\s+(an?\s+)?/i, "").trim() || "Untitled";
  const title = bare.charAt(0).toUpperCase() + bare.slice(1);
  if (spec.action === "daily") {
    await openToday("tab");
    const d = activeNote()?.doc;
    if (!d) return;
    const text = spec.template ? await templateText(spec.template, d.name) : "";
    replaceText(d, d.text.replace(/\s*$/, "") + "\n\n" + text);
    return;
  }
  if (spec.action === "new-note" || spec.action === "new") {
    const folder = cleanFolder(spec.folder) || newNoteFolder();
    const name = `${title} ${isoDay()}`;
    try {
      const text = spec.template ? await templateText(spec.template, name) : "";
      const path = await api.createFile(folder, name, "md", text);
      await refreshTree();
      await openNote(path, "tab", undefined, { mode: "source" });
    } catch (e) {
      toast(`Couldn't create the note: ${e}`, "error");
    }
    return;
  }
  toast(`This button's action “${spec.action}” isn't one Marrow knows. Use append, new-note or daily.`, "error");
}

/** Picks an image (e.g. a cover); copies it into the vault if it's outside. */
async function pickImage(): Promise<string | null> {
  const picked = await openDialog({
    title: "Choose an image",
    defaultPath: vaultRoot,
    multiple: false,
    filters: [{ name: "Images", extensions: ["png", "jpg", "jpeg", "gif", "webp", "svg", "avif", "bmp"] }],
  });
  if (typeof picked !== "string") return null;
  try {
    const path = await api.importFile(picked, "attachments");
    void refreshTree();
    return path;
  } catch (e) {
    toast(`Couldn't use that image: ${e}`, "error");
    return null;
  }
}

// --------------------------------------------------------- rename and move

/** Renames a note, canvas or folder in place. False if it didn't happen. */
async function renamePath(path: string, newName: string): Promise<boolean> {
  const name = newName.replace(/\s+/g, " ").trim();
  const node = tree.find(path);
  const current = node?.kind === "Folder" ? node.name : displayName(path);
  if (!name || name === current) return false;
  if (/[\\/:*?"<>|]/.test(name)) {
    toast('A name can\'t contain any of \\ / : * ? " < > |', "error");
    return false;
  }
  await saveUnder(path);
  try {
    // Notes and canvases keep their extension.
    const ext = isCanvasPath(path) && !name.toLowerCase().endsWith(".canvas") ? ".canvas" : "";
    afterMove(path, await api.rename(path, name + ext), "Renamed");
    return true;
  } catch (e) {
    toast(String(e), "error");
    return false;
  }
}

async function movePath(path: string, folder: string) {
  if (parentOf(path) === folder) return;
  await saveUnder(path);
  try {
    afterMove(path, await api.movePath(path, folder), "Moved");
  } catch (e) {
    toast(String(e), "error");
  }
}

// --------------------------------------------------------- version history

async function showHistory(path: string) {
  // Unsaved edits count as "now" and are saved first, so nothing is lost.
  const d = findDoc(path);
  if (d?.dirty) await save(d);
  void openHistory({
    path,
    current: () => findDoc(path)?.text ?? "",
    restore: (text) => {
      const doc = findDoc(path);
      if (doc) replaceText(doc, text);
      else void editNote(path, () => text).catch((e) => toast(`Couldn't restore: ${e}`, "error"));
    },
    toast,
  });
}

/** The time machine, on a note or (path null) the whole vault. */
async function timeMachine(path: string | null) {
  for (const d of docsInUse()) if (d.dirty) await save(d);
  await openTimeMachine({
    path,
    current: async (p) => findDoc(p)?.text ?? (await api.readNote(p)),
    restore: async (p, text) => {
      const doc = findDoc(p);
      if (doc) replaceText(doc, text);
      else await editNote(p, () => text);
      await openPath(p, "replace");
    },
    bringBack: async (p, text) => {
      const slash = p.lastIndexOf("/");
      const folder = slash >= 0 ? p.slice(0, slash) : "";
      const name = displayName(p);
      const made = await api.createFile(folder, name, "md", text).catch(() => api.createFile("", name, "md", text));
      await openPath(made, "replace");
    },
    open: (p) => void openPath(p, "replace"),
    toast,
  });
}

// ------------------------------------------------------ unlinked mentions

/** Turns a plain-text mention in another note into a link to `target`. */
async function linkMention(m: Mention, target: string) {
  const name = displayName(target);
  // The short name, unless from that note it would lead somewhere else.
  const resolved = await api.resolveLink(name, m.path).catch(() => null);
  const ref = resolved === target ? name : target.replace(/\.md$/i, "");
  const link = m.matched === name || m.matched === ref ? `[[${ref}]]` : `[[${ref}|${m.matched}]]`;
  try {
    await editNote(m.path, (text) => {
      const lines = text.split("\n");
      const line = lines[m.line];
      if (line === undefined) return text;
      let at = line.slice(m.ch, m.ch + m.len) === m.matched ? m.ch : line.indexOf(m.matched);
      if (at < 0) at = line.toLowerCase().indexOf(m.matched.toLowerCase());
      if (at < 0) return text;
      lines[m.line] = line.slice(0, at) + link + line.slice(at + m.matched.length);
      return lines.join("\n");
    });
    toast(`Linked “${m.matched}” in “${m.name}”.`);
  } catch (e) {
    toast(`Couldn't add the link: ${e}`, "error");
  }
}

// ---------------------------------------------------------------- stars

function setStarred(list: string[]) {
  vaultSettings = { ...vaultSettings, starred: list };
  void api.setVaultSettings(vaultSettings).catch((e) => toast(`Couldn't save the star: ${e}`, "error"));
  tree.render();
}

function toggleStar(path: string) {
  const list = vaultSettings.starred ?? [];
  const on = list.includes(path);
  setStarred(on ? list.filter((p) => p !== path) : [...list, path]);
  toast(on ? `Removed “${displayName(path)}” from Starred.` : `Starred “${displayName(path)}”.`);
}

async function saveUnder(path: string) {
  for (const d of allDocs()) if (d.path === path || d.path.startsWith(path + "/")) await save(d);
  if (canvasView.path && (canvasView.path === path || canvasView.path.startsWith(path + "/"))) await canvasView.flush();
}

const movedPath = (p: string, from: string, to: string) =>
  p === from ? to : p.startsWith(from + "/") ? to + p.slice(from.length) : null;

function afterMove(oldPath: string, r: RenameReport, verb: string) {
  pathMoved(oldPath, r.newPath);
  seenMoved(vaultRoot, oldPath, r.newPath);
  void refreshTree();
  const n = r.linksUpdated;
  const m = r.filesUpdated.length;
  toast(n === 0 ? `${verb}.` : `${verb}. Updated ${n} ${n === 1 ? "link" : "links"} in ${m} ${m === 1 ? "note" : "notes"}.`);
  const k = r.linksSkipped ?? 0;
  if (k > 0)
    toast(`${k} ${k === 1 ? "link wasn't" : "links weren't"} updated because another app was changing ${k === 1 ? "that note" : "those notes"} at the same moment. Search for the old name to fix ${k === 1 ? "it" : "them"}.`, "error");
}

/** Points everything open at a renamed path. Safe to call twice. */
function pathMoved(from: string, to: string) {
  docsMoved(from, to);
  for (const t of tabs) {
    if (t.kind === "canvas") {
      const p = movedPath(t.path, from, to);
      if (p) t.path = p;
    }
  }
  if (canvasView.path) {
    const p = movedPath(canvasView.path, from, to);
    if (p) canvasView.renamed(p);
  }
  recent = recent.map((p) => movedPath(p, from, to) ?? p);
  const stars = (vaultSettings.starred ?? []).map((p) => movedPath(p, from, to) ?? p);
  if (stars.some((p, i) => p !== vaultSettings.starred[i])) setStarred(stars);
  store(`marrow.recent.${vaultRoot}`, recent);
  tree.moved(from, to);
  primary.pathChanged();
  splitPane.pathChanged();
  renderTabs();
  showStage();
  persistSession();
}

/** Paths being moved to the trash by Marrow itself (not deleted outside). */
let trashing: string[] = [];

async function trashPath(path: string, isFolder: boolean) {
  const what = isFolder ? `the folder “${displayName(path)}” and everything in it` : `“${displayName(path)}”`;
  const ok = await ask(`Move ${what} to the vault trash?\n\nYou can get it back from the .marrow/trash folder.`, {
    title: "Move to trash",
    kind: "warning",
    okLabel: "Move to trash",
    cancelLabel: "Cancel",
  });
  if (!ok) return;
  const gone = (p: string) => p === path || p.startsWith(path + "/");
  // Save first, so the trash holds the latest text.
  stashActive();
  const failed = (await saveAll()).filter((d) => gone(d.path));
  if (failed.length && !(await settleEdits("Move to trash", failed))) return;
  if (canvasView.path && gone(canvasView.path) && !(await canvasView.flush())) return;
  trashing = [...trashing, path];
  try {
    await api.trash(path);
  } catch (e) {
    // Nothing was moved: everything stays open as it was.
    trashing = trashing.filter((t) => t !== path);
    toast(`Couldn't move it to the trash: ${e}`, "error");
    return;
  }
  for (let i = tabs.length - 1; i >= 0; i--) {
    const t = tabs[i];
    const p = t.kind === "note" ? t.doc.path : t.kind === "canvas" ? t.path : null;
    if (p && gone(p)) {
      tabs.splice(i, 1);
      if (i < active) active--;
      else if (i === active) active = -1;
    }
  }
  if (split && gone(split.doc.path)) {
    split = null;
    splitPane.clear();
  }
  if (canvasView.path && gone(canvasView.path)) canvasView.path = null;
  if ((vaultSettings.starred ?? []).some(gone)) setStarred(vaultSettings.starred.filter((p) => !gone(p)));
  forget(path);
  // The watcher reports the move a moment later; ignore it until then.
  window.setTimeout(() => (trashing = trashing.filter((t) => t !== path)), 3000);
  if (active < 0 && tabs.length) await activateTab(Math.min(tabs.length - 1, 0));
  else if (active < 0) {
    primary.clear();
    inspector.clear();
  }
  showStage();
  renderTabs();
  persistSession();
  await refreshTree();
}

// ------------------------------------------------------------- following

async function followLink(target: string, from: NotePane | null, inSplit: boolean) {
  const source = from?.path ?? activeNote()?.doc.path ?? "";
  if (!target) return;
  try {
    const path = await api.followLink(target, source);
    if (inSplit) return openInSplit(path);
    if (from === splitPane) return openInSplit(path);
    await openPath(path, "replace");
  } catch (e) {
    toast(`Couldn't open [[${target}]]: ${e}`, "error");
  }
}

function openTag(tag: string) {
  setSide("tags", true);
  tagsView.select(tag.replace(/^#/, ""));
}

// ------------------------------------------------------------------ sidebar

function setSide(v: typeof side, open = true) {
  side = v;
  sideOpen = open;
  $("app").classList.toggle("no-sidebar", !sideOpen);
  for (const name of ["files", "search", "tags", "tasks", "calendar"] as const) {
    $(`side-${name}`).hidden = name !== v;
    $(`rail-${name}`).setAttribute("aria-pressed", String(sideOpen && name === v));
  }
  if (v === "tags" && sideOpen) void tagsView.refresh();
  if (v === "tasks" && sideOpen) void tasksView.refresh();
  if (v === "calendar" && sideOpen) void calendarView.refresh();
  persistSession();
}

function railSide(v: typeof side) {
  if (side === v && sideOpen) setSide(v, false);
  else {
    setSide(v, true);
    if (v === "search") searchView.focus();
  }
}

// ------------------------------------------------------------------- tree

async function refreshTree() {
  tree.set(await api.tree());
  if (side === "calendar" && sideOpen) void calendarView?.refresh();
}

// ------------------------------------------------------------------ graph

function ensureGraph() {
  graphPanel ??= new GraphPanel($("graph-view"), {
    open: (n, inSplit) => void openFromGraph(n, inSplit),
    currentNote: () => [...tabs].reverse().find((t): t is NoteTab => t.kind === "note")?.doc.path ?? null,
  });
}

async function refreshGraph() {
  if (!graphPanel || activeTab()?.kind !== "graph") return;
  await graphPanel.refresh();
}

async function openFromGraph(n: GraphNode, inSplit = false) {
  if (n.kind === "note") return inSplit ? openInSplit(n.id) : openNote(n.id, "replace");
  if (n.kind === "tag") return openTag(n.id.slice(1));
  if (n.kind === "unresolved") return followLink(n.id.slice(1), null, false);
  toast("Attachments can't be opened in Marrow yet.");
}

// ---------------------------------------------------------------- session

interface Session {
  tabs: ({ kind: "note"; path: string; mode: Mode; anchor: number; scrollTop: number } | { kind: "graph" } | { kind: "canvas"; path: string })[];
  active: number;
  split: { path: string; mode: Mode } | null;
  side: typeof side;
  sideOpen: boolean;
}

let sessionTimer: number | undefined;
function persistSession() {
  window.clearTimeout(sessionTimer);
  sessionTimer = window.setTimeout(writeSession, 300);
}

function writeSession() {
  if (!vaultRoot) return;
  stashActive();
  const s: Session = {
    tabs: tabs.map((t) => {
      if (t.kind === "note") {
        const live = t === activeTab() && primary.doc === t.doc;
        const st = live ? primary.view.state : t.state;
        return { kind: "note", path: t.doc.path, mode: live ? primary.mode : t.mode, anchor: st?.selection.main.head ?? 0, scrollTop: live ? primary.scrollTop : t.scrollTop };
      }
      return t.kind === "canvas" ? { kind: "canvas", path: t.path } : { kind: "graph" };
    }),
    active,
    split: split ? { path: split.doc.path, mode: splitPane.mode } : null,
    side,
    sideOpen,
  };
  // stashActive() handed the state to the tab; the pane still owns it.
  const t = activeNote();
  if (t && primary.doc === t.doc) t.state = null;
  store(`marrow.session.${vaultRoot}`, s);
}

async function restoreSession() {
  const s = storage<Partial<Session> | null>(`marrow.session.${vaultRoot}`, null);
  setSide(s?.side ?? "files", s?.sideOpen ?? innerWidth >= 1000);
  if (!s || !Array.isArray(s.tabs)) {
    showStage();
    return;
  }
  const restored: Tab[] = [];
  let act = s.active ?? 0;
  for (const [i, t] of s.tabs.entries()) {
    if (t.kind === "graph") restored.push({ kind: "graph" });
    else if (t.kind === "canvas") {
      if (tree.find(t.path)) restored.push({ kind: "canvas", path: t.path });
      else if (i <= act) act--;
    } else if (t.kind === "note") {
      const text = await api.readNote(t.path).catch(() => null);
      if (text === null) {
        if (i <= act) act--;
        continue;
      }
      const doc = adoptDoc(t.path, text);
      const state = makeState(text, primary.hooks);
      const anchor = Math.min(t.anchor ?? 0, text.length);
      restored.push({ kind: "note", doc, state: state.update({ selection: { anchor } }).state, scrollTop: t.scrollTop ?? 0, mode: t.mode ?? prefs.defaultMode });
    }
  }
  tabs = restored;
  active = -1;
  if (tabs.length) await activateTab(Math.max(0, Math.min(act, tabs.length - 1)));
  if (s.split) {
    const text = await api.readNote(s.split.path).catch(() => null);
    if (text !== null) {
      const doc = adoptDoc(s.split.path, text);
      split = { doc, mode: s.split.mode };
      showStage();
      splitPane.show(doc, { mode: s.split.mode });
      primary.focus();
    }
  }
  showStage();
  renderTabs();
}

// ------------------------------------------------------- external changes

let refreshTimer: number | undefined;

async function onEngineEvents(events: EngineEvent[]) {
  // Database views refresh themselves from this.
  window.dispatchEvent(new Event("marrow-vault-changed"));
  let treeChanged = false;
  for (const ev of events) {
    if (ev.type !== "vault") continue;
    treeChanged = true;
    const v = ev.event;
    if (v.type === "created" || v.type === "renamed") checkConflictsSoon();
    if (v.type === "renamed") {
      pathMoved(v.from, v.to);
    } else if (v.type === "deleted") {
      const d = findDoc(v.path);
      if (d && !trashing.some((t) => d.path === t || d.path.startsWith(t + "/"))) {
        // A background tab whose note vanished just closes; a visible one stays.
        const visible = primary.doc === d || splitPane.doc === d;
        if (visible) onDiskDeleted(d);
        else {
          const i = tabs.findIndex((t) => t.kind === "note" && t.doc === d);
          if (i >= 0) {
            d.dirty = false;
            await closeTab(i);
          }
        }
      }
    } else if (v.type === "modified") {
      const d = findDoc(v.path);
      if (d) await onDiskChanged(d);
      if (canvasView.path === v.path) void canvasView.onDiskChanged();
    }
  }
  window.clearTimeout(refreshTimer);
  refreshTimer = window.setTimeout(() => {
    if (treeChanged) void refreshTree();
    void refreshLocked();
    void primary.refresh();
    if (split) void splitPane.refresh();
    const t = activeNote();
    if (t) void inspector.show(t.doc.path, t.doc.text);
    void refreshGraph();
    searchView.refresh();
    if (side === "tags" && sideOpen) void tagsView.refresh();
    if (side === "tasks" && sideOpen) void tasksView.refresh();
    void refreshStats();
    if (activeTab() === undefined) void renderStart();
  }, 200);
}

// -------------------------------------------------------------- commands

function commands(): Command[] {
  const cmd = (label: string, icon: string, run: () => void, hint = ""): Command => ({
    label,
    icon,
    hint,
    run,
  });
  const fromPlugins = pluginCommands().map((c) => cmd(c.label, c.icon, c.run));
  return [
    cmd("New note", "note-pencil", () => void newNote(), kbd("N")),
    cmd("Open today's daily note", "calendar-blank", () => void openToday(), kbd("D")),
    cmd("Show the calendar", "calendar-dots", () => setSide("calendar", true)),
    cmd("Weekly review", "sun-horizon", () => void openWeeklyReview(resurfaceApp)),
    cmd("Organize daily notes into year and month folders", "folders", () => void organizeDaily()),
    cmd("Open graph view", "graph", () => void openGraph(), kbd("G")),
    cmd("Open canvas", "squares-four", () => void openSomeCanvas()),
    cmd("Toggle split pane", "columns", () => void toggleSplit(), kbd("\\")),
    cmd("Toggle preview / source", "code", () => toggleMode(), kbd("E")),
    cmd("Toggle inspector", "sidebar-simple", () => toggleInspector()),
    cmd("Focus mode", "arrows-out", () => toggleFocus(), "F11"),
    cmd("Quick capture", "lightning", () => quickCapture(), prefs.captureKey ? prefs.captureKey.replace("CommandOrControl", isMac ? "⌘" : "Ctrl") : ""),
    cmd("Show all tasks", "check-square", () => setSide("tasks")),
    cmd("Remind me about this task", "bell-simple-plus", () => void remindHere()),
    cmd("Find and replace in all notes", "magnifying-glass", () => {
      setSide("search");
      searchView.showReplace(true);
      searchView.focus();
    }),
    cmd("Search all notes", "magnifying-glass", () => (setSide("search"), searchView.focus()), kbd("F", true)),
    cmd("Open settings", "gear-six", () => openSettings(), kbd(",")),
    cmd("Help: how Marrow works", "question", () => openHelp()),
    cmd("New canvas", "squares-four", () => void newCanvas()),
    cmd("New folder", "folder-simple-plus", () => void newFolder(newNoteFolder())),
    cmd("Rename this note", "pencil-simple", () => primary.doc && primary.focusTitle()),
    cmd("Version history of this note", "clock-counter-clockwise", () => {
      const d = focusedPane().doc;
      if (d) void showHistory(d.path);
      else toast("Open a note first.");
    }),
    cmd("Time machine: this note on any past date", "clock-clockwise", () => {
      const d = focusedPane().doc;
      if (d) void timeMachine(d.path);
      else toast("Open a note first.");
    }),
    cmd("Time machine: the whole vault on any past date", "clock-clockwise", () => void timeMachine(null)),
    cmd("Lock this note with a password…", "lock-simple", () => {
      const d = focusedPane().doc;
      if (!d) return toast("Open a note first.");
      if (isLockedNote(d.path)) return toast("This note is already locked.");
      void lockPath(d.path, false);
    }),
    cmd("Remove the lock from this note…", "lock-simple-open", () => {
      const d = focusedPane().doc;
      if (!d || !isLockedNote(d.path)) return toast("This note isn't locked.");
      void unlockRemove(d.path, false);
    }),
    cmd("Lock all notes now", "lock-key", () => void lockAllNow()),
    cmd("Present this note as slides", "presentation-chart", () => presentNote()),
    cmd("Publish notes as a website…", "globe", () => publishSite("")),
    cmd("Star / unstar this note", "star", () => {
      const t = activeTab();
      const p = t?.kind === "note" ? t.doc.path : t?.kind === "canvas" ? t.path : null;
      if (p) toggleStar(p);
    }),
    cmd("Close all tabs", "x", () => void closeAllTabs()),
    cmd("Close split pane", "x-square", () => split && closeSplit()),
    cmd("Show this note in the file tree", "tree-structure", () => {
      const t = activeTab();
      const p = t?.kind === "note" ? t.doc.path : t?.kind === "canvas" ? t.path : null;
      if (p) {
        setSide("files");
        tree.reveal(p);
      }
    }),
    cmd("Toggle dark mode", "moon", () => setPref("theme", document.documentElement.dataset.theme === "dark" || (prefs.theme === "system" && matchMedia("(prefers-color-scheme: dark)").matches) ? "light" : "dark")),
    cmd("Import from Notion, Evernote, OneNote…", "tray-arrow-down", () => importNotes()),
    cmd("Move from Notion (step by step)…", "notion-logo", () => moveFrom("notion")),
    cmd("Move from Evernote (step by step)…", "notebook", () => moveFrom("evernote")),
    cmd("Open another vault", "folder-open", () => void chooseVault()),
    cmd("Create a new vault", "folder-simple-plus", () => void createVaultDialog(openVault)),
    cmd("AI: Ask about this note (summarize, key points, translate…)", "sparkle", () => aiAboutNote()),
    cmd("AI: Edit selection or write at cursor", "sparkle", () => aiInEditor(), kbd("J")),
    cmd("AI: Ask my notes", "sparkle", () => askNotes(), `${kbd("J", true)}`),
    cmd("AI settings", "sparkle", () => openSettings("ai")),
    cmd("Export this note to PDF…", "file-pdf", () => void exportNote("pdf")),
    cmd("Export this note to HTML…", "file-html", () => void exportNote("html")),
    cmd("Print this note…", "printer", () => void exportNote("print")),
    mirror ? cmd("Sync now", "arrows-clockwise", () => void api.syncNow()) : cmd("Sync this vault (cloud folder or server)", "cloud", () => startSyncSetup()),
    cmd("Customize: CSS snippets and plugins", "paint-brush", () => openSettings("customize")),
    ...fromPlugins,
  ];
}

/** The pane being worked in: the split one if it has focus. */
function focusedPane(): NotePane {
  return split && splitPane.root.contains(document.activeElement) ? splitPane : primary;
}

/** "Remind me" on the editor's current line (made a task if it isn't one). */
async function remindHere() {
  const pane = focusedPane();
  if (!pane.doc) return toast("Open a note first.");
  if (pane.mode !== "source") pane.toggleMode();
  const view = pane.view;
  const line = view.state.doc.lineAt(view.state.selection.main.head);
  const current = await reminderOf(line.text).catch(() => null);
  const coords = view.coordsAtPos(line.from);
  const anchor = h("span", { style: `position:fixed;left:${coords?.left ?? 200}px;top:${coords?.bottom ?? 200}px` });
  document.body.append(anchor);
  pickReminder(anchor, current, async (at) => {
    anchor.remove();
    const now = view.state.doc.lineAt(Math.min(line.from, view.state.doc.length));
    const next = await lineWithReminder(now.text, at);
    view.dispatch({ changes: { from: now.from, to: now.to, insert: next } });
    toast(at ? `Reminder set: ${whenLabel(at)}.` : "Reminder removed.");
    if (at) void maybeAskBackground(toast);
  });
  window.setTimeout(() => anchor.isConnected && !document.querySelector(".remind-pop") && anchor.remove(), 30_000);
}

function aiAboutNote() {
  const pane = focusedPane();
  const d = pane.doc;
  if (!d) return toast("Open a note first.");
  aiOnNote({
    title: displayName(d.path),
    text: () => d.text,
    append: (text) => replaceText(d, d.text.replace(/\s*$/, "") + "\n\n" + text + "\n"),
  });
}

/** Prints the note in focus, or saves it as PDF (through printing) or HTML. */
async function exportNote(as: "pdf" | "html" | "print") {
  const pane = focusedPane();
  if (!pane.doc) return toast("Open a note first.");
  const c = await pane.exportContent();
  if (!c) return;
  try {
    if (as === "html") {
      const path = await exportHtml(c.title, c.el);
      if (path) toast(`Saved “${path.split(/[\\/]/).pop()}”.`);
    } else {
      if (as === "pdf") toast("Choose “Save as PDF” (or Microsoft Print to PDF) as the printer.");
      await printNote(c.title, c.el);
    }
  } catch (e) {
    toast(`Couldn't export the note: ${e}`, "error");
  }
}

/** After an import: the new folder in the file tree, its first note open. */
async function showImported(folder: string) {
  await refreshTree();
  setSide("files");
  const first = tree.files().find((p) => p.startsWith(folder + "/") && isNotePath(p));
  if (first) {
    tree.reveal(first);
    void openPath(first, "replace");
  }
}

function moveFrom(from: SwitchFrom) {
  openSwitch({ done: (folder) => void showImported(folder), mobile: platform.mobile }, from);
}

function importNotes() {
  openImport({
    guided: moveFrom,
    done: (folder) =>
      void (async () => {
        await refreshTree();
        setSide("files");
        const first = tree.files().find((p) => p.startsWith(folder + "/") && isNotePath(p));
        if (first) {
          tree.reveal(first);
          await openNote(first, "replace");
        }
      })(),
  });
}

/** A question to the whole vault, answered from the notes with sources. */
function askNotes() {
  aiOnVault({
    openNote: (path, line) => void openPath(path, "replace", { line, ch: 0 }),
    saveNote: (question, text) =>
      void (async () => {
        const title = ("Q " + question.replace(/[\\/:*?"<>|#^[\]]/g, " ")).replace(/\s+/g, " ").trim().slice(0, 70);
        try {
          const path = await api.createFile(newNoteFolder(), title, "md", text);
          await refreshTree();
          await openNote(path, "tab");
        } catch (e) {
          toast(`Couldn't save the answer: ${e}`, "error");
        }
      })(),
  });
}

function aiInEditor() {
  const pane = focusedPane();
  if (!pane.doc) return toast("Open a note first.");
  if (pane.mode !== "source") pane.setMode("source");
  window.setTimeout(() => aiOnEditor(pane.view, displayName(pane.doc!.path)), 0);
}

function toggleMode() {
  const focusSplit = splitPane.root.contains(document.activeElement);
  const pane = focusSplit && split ? splitPane : primary;
  if (pane.doc) pane.toggleMode();
}

// ------------------------------------------------------------- cloud sync

let mirror: MirrorStatus | null = null;
/** Counts sync events, so an older status fetched meanwhile doesn't overwrite a newer one. */
let mirrorEvents = 0;
/** The sync problem a banner was last shown for, so it isn't repeated. */
let syncProblem = "";

function renderSync() {
  const el = $("sync-state");
  el.hidden = !mirror;
  if (!mirror) return;
  const s = mirror;
  const state = s.stop ? "error" : s.syncing ? "busy" : "ok";
  el.dataset.state = state;
  el.replaceChildren(
    icon(state === "error" ? "cloud-warning" : state === "busy" ? "cloud-arrow-up" : "cloud-check"),
    h("span.label", null, state === "error" ? "Sync paused" : state === "busy" ? "Syncing…" : "Synced"),
  );
  el.title = syncLine(s) + (state === "error" ? "" : "\nClick to sync now");
  updateSyncLine(syncLine(s));
}

function onMirror(s: MirrorStatus, report: SyncReport | null) {
  mirrorEvents++;
  mirror = s;
  renderSync();
  if (s.stop) {
    if (syncProblem !== s.stop.kind) {
      syncProblem = s.stop.kind;
      showSyncProblem(s);
    }
  } else if (syncProblem) {
    syncProblem = "";
    if ($("banner").dataset.sync) hideBanner();
  }
  if (!report) return;
  if (report.merged.length) {
    const first = displayName(report.merged[0]);
    const more = report.merged.length - 1;
    toast(`Merged edits to “${first}”${more ? ` and ${more} more` : ""} from ${s.label}.`);
  }
  if (report.conflictCopies.length) void checkConflicts();
  if (report.errors.length) toast(`Couldn't sync ${report.errors.length === 1 ? "a file" : `${report.errors.length} files`}: ${report.errors[0]}`, "error");
}

function showSyncProblem(s: MirrorStatus) {
  const st = s.stop;
  if (!st) return;
  if (st.kind === "missing") {
    showBanner(`Can't find the ${s.label} folder. Is ${s.label} running? Sync starts again by itself when it's back.`, "warn");
  } else if (st.kind === "offline") {
    // Being offline is normal on a phone: the status shows it, and sync resumes by itself.
    return;
  } else if (st.kind === "signIn") {
    showBanner(s.message ?? "Sign in to sync again.", "warn", [{ label: "Sync settings", run: () => openSettings("vault") }]);
  } else if (st.kind === "looksEmpty" || st.kind === "manyDeletes") {
    const where = st.side === "remote" ? `your ${s.label} folder` : "this vault";
    showBanner(`${s.message} Have a look at ${where}; if that's what you meant, sync anyway.`, "warn", [
      { label: "Sync anyway", run: () => void api.syncNow(true) },
      { label: "Sync settings", run: () => openSettings("vault") },
    ]);
  } else {
    showBanner(s.message ?? "Sync failed.", "error", [{ label: "Try again", run: () => void api.syncNow() }]);
  }
  $("banner").dataset.sync = "1";
}

async function refreshMirror() {
  const seen = mirrorEvents;
  const s = await api.mirrorStatus().catch(() => null);
  if (seen !== mirrorEvents && s) return;
  mirror = s;
  syncProblem = "";
  renderSync();
}

function startSyncSetup() {
  void setupSyncDialog(
    vaultName,
    () => {
      toast("Sync is on. Changes go both ways from now on.");
      void refreshMirror();
    },
    platform.mobile,
  );
}

async function stopSync() {
  await api.setMirror(null).catch((e) => toast(String(e), "error"));
  mirror = null;
  renderSync();
  toast("Sync is off. Both folders keep their files.");
}

// ------------------------------------------------------- conflict copies

const SOURCE_NAME = { dropbox: "Dropbox", onedrive: "OneDrive", gdrive: "Google Drive", syncthing: "Syncthing", marrow: "Sync" } as const;
let conflictTimer: number | undefined;

function checkConflictsSoon() {
  window.clearTimeout(conflictTimer);
  conflictTimer = window.setTimeout(() => void checkConflicts(), 1500);
}

async function checkConflicts() {
  const key = `marrow.conflicts.${vaultRoot}`;
  const kept = storage<string[]>(key, []);
  const list = (await api.conflictCopies().catch(() => [] as ConflictCopy[])).filter((c) => !kept.includes(c.copy));
  if (!list.length) return;
  const one = list.length === 1;
  const text = one
    ? `${SOURCE_NAME[list[0].source]} kept two versions of “${displayName(list[0].original)}” after it was edited in two places.`
    : `Your sync app kept two versions of ${list.length} notes after they were edited in two places.`;
  showBanner(text, "warn", [
    { label: one ? "Merge them" : "Merge all", run: () => void mergeConflicts(list) },
    {
      label: "Compare",
      run: async () => {
        await openPath(list[0].original, "tab");
        await openPath(list[0].copy, "split");
      },
    },
    { label: "Keep both", run: () => store(key, [...kept, ...list.map((c) => c.copy)]) },
  ]);
}

async function mergeConflicts(list: ConflictCopy[]) {
  let both = 0;
  for (const c of list) {
    try {
      both += await api.resolveConflict(c.copy, c.original);
    } catch (e) {
      toast(`Couldn't merge “${displayName(c.copy)}”: ${e}`, "error");
      return;
    }
  }
  const what = list.length === 1 ? `Merged into “${displayName(list[0].original)}”.` : `Merged ${list.length} notes.`;
  toast(`${what} ${both ? "Where both versions changed the same lines, both are kept. " : ""}The copies are in the trash.`);
}

// ------------------------------------------------------------------ vault

async function chooseVault() {
  if (platform.mobile) {
    toast(platform.os === "ios" ? "Your notes are in the Marrow folder: open the Files app › On My iPhone/iPad › Marrow." : "Your notes are kept in Marrow's own folder on this device.");
    return;
  }
  const picked = await openDialog({ directory: true, multiple: false, title: "Open a folder of notes" });
  if (typeof picked === "string") await openVault(picked);
}

async function openVault(path: string) {
  if (canvasView && !(await settleEdits("Switch vaults"))) return;
  window.clearTimeout(sessionTimer);
  writeSession();
  $("welcome-error").hidden = true;
  let info;
  try {
    info = await api.openVault(path);
  } catch (e) {
    showWelcome(String(e));
    return;
  }
  for (const d of allDocs()) forget(d.path);
  tabs = [];
  active = -1;
  split = null;
  primary.clear();
  splitPane.clear();
  inspector.clear();
  vaultRoot = info.root;
  vaultName = info.name;
  vaultSettings = await api.getVaultSettings().catch(() => vaultSettings);
  usePropertyTypes(vaultSettings.propertyTypes, (types) => {
    vaultSettings = { ...vaultSettings, propertyTypes: types };
    void api.setVaultSettings(vaultSettings).catch((e) => toast(`Couldn't save the property type: ${e}`, "error"));
  });
  recent = storage<string[]>(`marrow.recent.${vaultRoot}`, []);
  $("vault-name").textContent = info.name;
  $("vault-name").title = `${info.root}\nClick to open a different folder`;
  const home = await homeDir().catch(() => "");
  $("vault-path").textContent = home && info.root.startsWith(home) ? "~" + info.root.slice(home.replace(/[\\/]+$/, "").length) : info.root;
  $("vault-path").title = info.root;
  $("welcome").hidden = true;
  $("app").hidden = false;
  tree.setVault(vaultRoot);
  searchView.reset();
  tagsView.selected = null;
  graphPanel?.destroy();
  graphPanel = null;
  await api.lockNow().catch(() => {});
  void applySnippets(vaultSettings.snippets ?? []);
  void startPlugins(vaultSettings.plugins ?? []);
  for (const msg of info.recovered ?? []) toast(msg);
  await refreshLocked();
  await refreshTree();
  void refreshStats();
  await restoreSession();
  renderTabs();
  showStage();
  void refreshMirror();
  checkConflictsSoon();
}

function showWelcome(error?: string) {
  $("app").hidden = true;
  $("welcome").hidden = false;
  const el = $("welcome-error");
  el.hidden = !error;
  if (error) el.textContent = error;
}

// ------------------------------------------------------------------- boot

function bindKeys() {
  document.addEventListener("keydown", (e) => {
    if (e.key === "F11" && !$("app").hidden) {
      e.preventDefault();
      toggleFocus();
    }
  });
  // A file dropped where nothing takes it mustn't replace the app with the file.
  for (const type of ["dragover", "drop"]) {
    window.addEventListener(type, (e) => {
      if (Array.from((e as DragEvent).dataTransfer?.types ?? []).includes("Files")) e.preventDefault();
    });
  }
  document.addEventListener("keydown", (e) => {
    if (e.key === "Escape") {
      hideMenu();
      if (paletteOpen()) closePalette();
      if (settingsOpen()) closeSettings();
      return;
    }
    const mod = e.ctrlKey || e.metaKey;
    if (!mod || $("app").hidden || e.altKey) return;
    if (e.defaultPrevented) return;
    const k = e.key.toLowerCase();
    const run = (fn: () => unknown) => {
      e.preventDefault();
      void fn();
    };
    // In Source the editor handles it (selection or cursor); elsewhere, the whole note.
    if (k === "j" && e.shiftKey) return run(() => askNotes());
    if (k === "j" && !e.shiftKey) return run(() => (activeNote() ? aiAboutNote() : undefined));
    if (k === "k" || k === "o" || k === "p") run(() => (paletteOpen() ? closePalette() : openPalette()));
    else if (k === "t") run(() => openPalette({ newTab: true }));
    else if (k === "s") run(() => primary.doc && forceSave(primary.doc));
    else if (k === "n" && !e.shiftKey) run(() => newNote());
    else if (k === "e") run(() => toggleMode());
    else if (k === "d" && !e.shiftKey) run(() => openToday());
    else if (k === "g") run(() => openGraph());
    else if (k === "\\") run(() => toggleSplit());
    else if (k === ",") run(() => (settingsOpen() ? closeSettings() : openSettings()));
    else if (k === "f" && e.shiftKey) {
      run(() => {
        setSide("search");
        const v = primary.view;
        const sel = primary.mode === "source" ? v.state.sliceDoc(v.state.selection.main.from, v.state.selection.main.to) : String(window.getSelection() ?? "");
        searchView.focus(sel && !sel.includes("\n") ? sel : undefined);
      });
    } else if (k === "w") run(() => active >= 0 && closeTab(active));
    else if ((e.code === "Tab" || k === "tab" || k === "pageup" || k === "pagedown") && tabs.length > 1) {
      // Ctrl+Tab / Ctrl+PageDown: next tab. Ctrl+Shift+Tab / Ctrl+PageUp: previous.
      const step = k === "pageup" || (k !== "pagedown" && e.shiftKey) ? -1 : 1;
      run(() => activateTab((active + step + tabs.length) % tabs.length));
    }
  });
}

function bindChrome() {
  $("rail-files").addEventListener("click", () => railSide("files"));
  $("rail-search").addEventListener("click", () => railSide("search"));
  $("rail-tags").addEventListener("click", () => railSide("tags"));
  $("rail-tasks").addEventListener("click", () => railSide("tasks"));
  $("rail-calendar").addEventListener("click", () => railSide("calendar"));
  $("rail-ask").addEventListener("click", () => askNotes());
  $("capture-fab").addEventListener("click", () => quickCapture());
  $("start-ask").addEventListener("click", () => askNotes());
  $("rail-today").addEventListener("click", () => void openToday());
  $("rail-graph").addEventListener("click", () => void openGraph());
  $("rail-canvas").addEventListener("click", () => void openSomeCanvas());
  $("rail-palette").addEventListener("click", () => openPalette());
  $("rail-settings").addEventListener("click", () => openSettings());
  $("rail-help").addEventListener("click", () => openHelp());
  $("new-note").addEventListener("click", () => void newNote());
  $("vault-name").addEventListener("click", () => void chooseVault());
  $("open-vault").addEventListener("click", () => void chooseVault());
  $("create-vault").addEventListener("click", () => void createVaultDialog(openVault));
  $("sync-state").addEventListener("click", () => {
    if (mirror?.stop) showSyncProblem(mirror);
    else void api.syncNow();
  });
  // Keep "synced 2 min ago" current.
  window.setInterval(renderSync, 30_000);
  $("jump").addEventListener("click", () => openPalette());
  $("split-btn").addEventListener("click", () => void toggleSplit());
  $("inspector-btn").addEventListener("click", () => toggleInspector());
  $("start-new").addEventListener("click", () => void newNote());
  $("start-today").addEventListener("click", () => void openToday());
  $("start-search").addEventListener("click", () => openPalette());

  // Shortcut hints follow the platform.
  $("jump-kbd").textContent = isMac ? "⌘K" : "Ctrl K";
  for (const el of document.querySelectorAll<HTMLElement>("[data-kbd]")) {
    const k = el.dataset.kbd!;
    el.textContent = k.startsWith("shift+") ? kbd(k.slice(6), true) : kbd(k);
  }
  const tips: [string, string][] = [
    ["rail-search", `Search (${kbd("F", true)})`],
    ["rail-today", `Today's note (${kbd("D")})`],
    ["rail-ask", `Ask your notes (${kbd("J", true)})`],
    ["rail-graph", `Graph view (${kbd("G")})`],
    ["rail-palette", `Search and commands (${kbd("K")})`],
    ["rail-settings", `Settings (${kbd(",")})`],
    ["new-note", `New note (${kbd("N")})`],
    ["split-btn", `Split pane (${kbd("\\")})`],
  ];
  for (const [id, t] of tips) $(id).title = t;

  // Narrow windows: the inspector opens over the note only when asked.
  let wasNarrow = innerWidth < 1200;
  window.addEventListener("resize", () => {
    const narrow = innerWidth < 1200;
    if (narrow !== wasNarrow) {
      wasNarrow = narrow;
      inspectorOnPhone = false;
      showStage();
    } else $("inspector").classList.toggle("floating", narrow);
  });
  // A crowded tab bar scrolls sideways with the wheel.
  $("tabs").addEventListener(
    "wheel",
    (e) => {
      if (Math.abs(e.deltaY) <= Math.abs(e.deltaX)) return;
      e.preventDefault();
      $("tabs").scrollLeft += e.deltaY;
    },
    { passive: false },
  );
}

let editTimer: number | undefined;

/** What plugins can do (each call is checked against their permissions first). */
function pluginHooks() {
  const current = () => {
    const pane = focusedPane();
    if (pane.doc && (isLockedNote(pane.doc.path) || lockedFolderOf(pane.doc.path) !== null)) throw new Error("This note is locked, so plugins can't use it.");
    return pane.doc ? { pane, doc: pane.doc } : null;
  };
  initPlugins({
    note: () => {
      const c = current();
      if (!c) return null;
      const sel = c.pane.mode === "source" ? c.pane.view.state.sliceDoc(c.pane.view.state.selection.main.from, c.pane.view.state.selection.main.to) : "";
      return { path: c.doc.path, title: displayName(c.doc.path), text: c.doc.text, selection: sel };
    },
    insert: (text) => {
      const c = current();
      if (!c) return false;
      if (c.pane.mode === "source") {
        const r = c.pane.view.state.selection.main;
        c.pane.view.dispatch({ changes: { from: r.from, to: r.to, insert: text }, selection: { anchor: r.from + text.length }, userEvent: "input.plugin", scrollIntoView: true });
      } else {
        const t = c.doc.text;
        replaceText(c.doc, t + (t.endsWith("\n") || !t ? "" : "\n") + text + "\n");
      }
      return true;
    },
    setText: (text) => {
      const c = current();
      if (!c) return false;
      replaceText(c.doc, text);
      return true;
    },
    notes: () => tree.files().filter((p) => isNotePath(p) && !isLockedNote(p) && lockedFolderOf(p) === null).map((p) => ({ path: p, title: displayName(p) })),
    read: async (path) => {
      if (isLockedNote(path) || lockedFolderOf(path) !== null) throw new Error("That note is locked.");
      return findDoc(path)?.text ?? (await api.readNote(path));
    },
    create: async (name, text) => {
      const path = await api.createNote(newNoteFolder(), name.replace(/[\\/:*?"<>|]/g, "-").slice(0, 120));
      if (text) await api.writeNote(path, text, "");
      await refreshTree();
      return path;
    },
    open: (path) => void openPath(path, "replace"),
    toast,
  });
}

async function boot() {
  platform = await api.appPlatform().catch(() => platform);
  document.documentElement.dataset.os = platform.os;
  document.documentElement.toggleAttribute("data-mobile", platform.mobile);
  applyPrefs();
  setLanguage(prefs.language);
  pluginHooks();
  onVimWrite(() => {
    const d = focusedPane().doc;
    if (d) void forceSave(d).then(() => toast(`Saved “${displayName(d.path)}”.`));
  });
  initLocks();
  setLockAll(() => lockAllNow(true));
  onLockList(() => {
    tree.render();
    primary.renderMeta();
    if (split) splitPane.renderMeta();
  });
  initDocs({
    onState: updateSaveState,
    banner: showBanner,
    hideErrorBanner: () => {
      if ($("banner").dataset.kind === "error") hideBanner();
    },
    toast,
    unlock: (path, folder) => unlockPath(path, folder),
  });

  const paneApp = {
    noteNames: () => tree.files().filter(isNotePath).map(displayName),
    toast: (text: string, kind?: "info" | "error") => toast(text, kind),
    vaultRoot: () => vaultRoot,
    openLink: (target: string, from: NotePane, inSplit: boolean) => void followLink(target, from, inSplit),
    openPath: (path: string, from: NotePane, inSplit: boolean) =>
      void (inSplit || from === splitPane ? openInSplit(path) : openPath(path, "replace")),
    openTag,
    rename: renamePath,
    dailyNav,
    onEdited: (pane: NotePane) => {
      if (pane !== primary) return;
      updateStatus();
      window.clearTimeout(editTimer);
      editTimer = window.setTimeout(() => {
        if (primary.doc) inspector.textChanged(primary.doc.text);
      }, 300);
    },
    onModeChanged: (pane: NotePane) => {
      if (pane === primary) {
        const t = activeNote();
        if (t) t.mode = pane.mode;
      } else if (split) split.mode = pane.mode;
      persistSession();
    },
    onCursor: () => persistSession(),
    onInfo: (pane: NotePane) => {
      if (pane === primary) updateStatus();
    },
    closeSplit,
    templates,
    templateText,
    runButton: (spec: ButtonSpec) => void runButton(spec),
    pickImage,
  };
  primary = new NotePane($("pane-primary"), "primary", paneApp);

  splitPane = new NotePane($("pane-split"), "split", paneApp);

  tree = new Tree($("tree"), {
    activePath: () => {
      const t = activeTab();
      return t?.kind === "note" ? t.doc.path : t?.kind === "canvas" ? t.path : null;
    },
    open: (path, how) => void openPath(path, how),
    rename: renamePath,
    move: (path, folder) => void movePath(path, folder),
    trash: (path, isFolder) => void trashPath(path, isFolder),
    newNote: (folder) => void newNote(folder),
    newFolder: (folder) => void newFolder(folder),
    newCanvas: (folder) => void newCanvas(folder),
    newDatabase: (folder) => void newDatabase(folder),
    reveal: (path) => {
      const root = vaultRoot.replace(/[\\/]+$/, "");
      const sep = root.includes("\\") ? "\\" : "/";
      void revealItemInDir(root + sep + path.split("/").join(sep)).catch((e) => toast(String(e), "error"));
    },
    dailyFolder: () => vaultSettings.dailyFolder.replace(/^\/+|\/+$/g, ""),
    starred: () => vaultSettings.starred ?? [],
    toggleStar,
    importNotes,
    lock: (path, isFolder) => void lockPath(path, isFolder),
    unlockRemove: (path, isFolder) => void unlockRemove(path, isFolder),
    publish: (folder) => publishSite(folder),
    history: (path) => void (async () => {
      await openPath(path, "replace");
      await showHistory(path);
    })(),
    timeMachine: (path) => void timeMachine(path),
    exportNote: (path, as) =>
      void (async () => {
        await openPath(path, "replace");
        await exportNote(as);
      })(),
  });

  const sideApp = { openNote: (path: string, at?: At, newTab?: boolean) => void openPath(path, newTab ? "tab" : "replace", at), toast };
  searchView = new SearchView(sideApp);
  tagsView = new TagsView(sideApp);
  calendarView = new CalendarView({
    dailyIndex,
    openDay: (day, newTab) => void openDay(day, newTab ? "tab" : "replace"),
    openNote: (path, newTab) => void openPath(path, newTab ? "tab" : "replace"),
    misplacedCount: () => misplacedDailyNotes().length,
    layout: dailyLayout,
    organize: organizeDaily,
  });
  tasksView = new TasksView({ openNote: sideApp.openNote, addTask: (text) => appendToToday(`- [ ] ${text}`, "Tasks"), toast });
  if (!platform.mobile) startReminders({ openNote: (p, at) => void openPath(p, "replace", at), toast });
  // From the browser extension (Marrow Clipper).
  void listen<{ path: string; title: string }>("clipped", (e) => toast(`Clipped “${e.payload.title}”.`));
  void listen<string>("open-note", (e) => void openPath(e.payload, "replace"));

  inspector = new Inspector({
    openPath: (path, at, inSplit) => void (inSplit ? openInSplit(path, at) : openPath(path, "replace", at)),
    openLink: (target, inSplit) => void followLink(target, primary, !!inSplit),
    jumpToLine: (line) => primary.scrollToLine(line, true),
    linkMention,
  });

  canvasView = new CanvasView($("canvas-view"), {
    vaultRoot: () => vaultRoot,
    openPath: (path, inSplit) => void (inSplit ? openInSplit(path) : openPath(path, isCanvasPath(path) ? "tab" : "tab")),
    toast,
  });

  initPalette({
    commands,
    recent: () => recent.filter((p) => tree.find(p)),
    source: () => activeNote()?.doc.path,
    open: (path, how, at) => void openPath(path, how, at),
    create: (name, how) => void createNamed(name, how),
  });

  initSettings({
    vaultName: () => vaultName,
    vaultRoot: () => vaultRoot,
    folders: () => tree.folders(),
    vaultSettings: () => vaultSettings,
    misplacedDaily: () => misplacedDailyNotes().length,
    organizeDaily,
    setVaultSettings: (s) => {
      const dailyChanged = s.dailyFolder !== vaultSettings.dailyFolder || s.dailyLayout !== vaultSettings.dailyLayout;
      vaultSettings = s;
      void api.setVaultSettings(s).catch((e) => toast(`Couldn't save the setting: ${e}`, "error"));
      if (dailyChanged) {
        tree.render();
        primary.pathChanged();
        renderTabs();
        if (side === "calendar" && sideOpen) void calendarView.refresh();
      }
    },
    openAnotherVault: () => void chooseVault(),
    importNotes,
    createVault: () => void createVaultDialog(openVault),
    mirror: () => mirror,
    setupSync: () => startSyncSetup(),
    stopSync: () => void stopSync(),
    revealVault: () => void revealItemInDir(vaultRoot).catch((e) => toast(String(e), "error")),
    customize: () => ({
      snippetsOn: () => vaultSettings.snippets ?? [],
      pluginsOn: () => vaultSettings.plugins ?? [],
      setSnippets: (on) => {
        vaultSettings = { ...vaultSettings, snippets: [...new Set(on)] };
        void api.setVaultSettings(vaultSettings).catch((e) => toast(`Couldn't save the setting: ${e}`, "error"));
        void applySnippets(vaultSettings.snippets);
      },
      setPlugins: (on) => {
        vaultSettings = { ...vaultSettings, plugins: [...new Set(on)] };
        void api.setVaultSettings(vaultSettings).catch((e) => toast(`Couldn't save the setting: ${e}`, "error"));
        void startPlugins(vaultSettings.plugins);
      },
      toast,
    }),
  });

  applyCaptureKey();
  void listen<boolean>("quick-capture", (e) => quickCapture(e.payload));
  let lang = prefs.language;
  onPrefs(() => {
    applyCaptureKey();
    if (prefs.language !== lang) setLanguage((lang = prefs.language));
    for (const p of [primary, splitPane]) {
      setLivePreview(p.view, prefs.livePreview);
      applyWriting(p.view);
    }
    showStage();
    graphPanel?.restyle();
  });
  matchMedia("(prefers-color-scheme: dark)").addEventListener("change", () => graphPanel?.restyle());

  bindChrome();
  bindKeys();

  // Save when the window loses focus and before it closes.
  window.addEventListener("blur", () => {
    void saveAll();
    void canvasView.flush();
    writeSession();
  });
  await getCurrentWindow().onCloseRequested(async (event) => {
    // Never close over text that isn't saved, unless the person says so.
    if (!(await settleEdits("Close Marrow"))) {
      event.preventDefault();
      return;
    }
    writeSession();
  });
  // Putting a note into a locked folder asks for its password first.
  setUnlockHandler((folder) => unlockPath(folder, folder));
  // Errors in the window go to the diagnostic log too (messages only).
  window.addEventListener("error", (e) => void logError(`window: ${e.message} at ${e.filename}:${e.lineno}`));
  window.addEventListener("unhandledrejection", (e) => void logError(`window: unhandled ${String(e.reason).slice(0, 300)}`));
  // Problems the backend wants the person to know about.
  void listen<string>("notice", (e) => toast(e.payload, "error"));

  await onVaultChanged((ev) => void onEngineEvents(ev));
  await onMirrorChanged(onMirror);

  const last = await api.lastVault();
  if (last) await openVault(last);
  // Phones and tablets have no folder picker: notes live in Marrow's own folder.
  else if (platform.mobile) await openVault(await api.defaultVault());
  else showWelcome();
}

void boot();
