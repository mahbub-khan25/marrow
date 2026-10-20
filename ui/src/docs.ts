// Open notes and their saving.
//
// A `Doc` is one note open somewhere: in a tab, in the split pane, or both.
// Every pane showing it reads and writes `doc.text`; the doc saves itself a
// moment after the last edit, refuses to overwrite a version it hasn't seen
// (merging instead), and keeps a backup when a save fails.
//
// Unsaved text is also kept as a draft (outside the vault, in the app's data
// folder) half a second after each edit, so a crash or power cut loses
// nothing: the next time the note opens, the draft is offered back. A
// successful save removes it.

import { api, displayName } from "./api";

/** Save this long after the last keystroke. */
const SAVE_DELAY_MS = 2000;
/** Keep a crash draft this long after the last keystroke. */
const DRAFT_DELAY_MS = 500;

export type SaveState = "saved" | "unsaved" | "saving" | "error";

/** Where a change came from, so a pane can skip its own edits. */
export type Origin = object | "disk" | "merge" | "app";

export class Doc {
  /** Text as last written to (or read from) disk. */
  saved: string;
  /** Text as it is now in the editor. */
  text: string;
  dirty = false;
  state: SaveState = "saved";
  timer: number | undefined;
  draftTimer: number | undefined;
  saving: Promise<void> | null = null;
  listeners = new Set<(origin: Origin) => void>();

  constructor(
    public path: string,
    text: string,
  ) {
    this.saved = text;
    this.text = text;
  }

  get name() {
    return displayName(this.path);
  }
}

export interface DocHost {
  /** Something about saving changed; update the status bar. */
  onState: () => void;
  banner: (text: string, kind: "warn" | "error", actions?: { label: string; run: () => void }[]) => void;
  hideErrorBanner: () => void;
  toast: (text: string, kind?: "info" | "error") => void;
  /** Asks for a locked note's (or folder's) password; true once entered. */
  unlock: (path: string, folder: string | null) => Promise<boolean>;
}

const docs = new Map<string, Doc>();
let host: DocHost;

export function initDocs(h: DocHost) {
  host = h;
}

export function findDoc(path: string): Doc | undefined {
  return docs.get(path);
}

/** The open doc for `path`, reading it from disk if it isn't open yet. */
export async function openDoc(path: string): Promise<Doc> {
  const have = docs.get(path);
  if (have) return have;
  const text = await api.readNote(path);
  // Opened twice at once: keep the first.
  const again = docs.get(path);
  if (again) return again;
  const d = new Doc(path, text);
  docs.set(path, d);
  return d;
}

/** A doc made from text already in hand (session restore). */
export function adoptDoc(path: string, text: string): Doc {
  const have = docs.get(path);
  if (have) return have;
  const d = new Doc(path, text);
  docs.set(path, d);
  return d;
}

export function allDocs(): Doc[] {
  return [...docs.values()];
}

/** Forgets docs nothing shows any more (saving them first). */
export async function releaseUnused(inUse: Set<Doc>) {
  for (const d of [...docs.values()]) {
    if (inUse.has(d)) continue;
    await save(d);
    if (!d.dirty) docs.delete(d.path);
  }
}

function setState(d: Doc, s: SaveState) {
  d.state = s;
  host.onState();
}

/** Listeners for a change to any open note (embeds of it redraw). */
export const anyDocChanged = new Set<(d: Doc) => void>();

function notify(d: Doc, origin: Origin) {
  for (const fn of d.listeners) fn(origin);
  for (const fn of anyDocChanged) fn(d);
}

/** A pane changed the text. */
export function edit(d: Doc, text: string, origin: Origin) {
  if (text === d.text) return;
  d.text = text;
  d.dirty = true;
  setState(d, "unsaved");
  notify(d, origin);
  schedule(d);
  window.clearTimeout(d.draftTimer);
  d.draftTimer = window.setTimeout(() => void keepDraft(d), DRAFT_DELAY_MS);
}

/** Keeps the unsaved text where a crash can't take it. */
async function keepDraft(d: Doc): Promise<boolean> {
  window.clearTimeout(d.draftTimer);
  if (!d.dirty || d.text === d.saved) return true;
  return api.keepDraft(d.path, d.text).then(
    () => true,
    () => false,
  );
}

/** Makes sure each doc's unsaved text is kept as a draft (before giving up on saving it). */
export async function keepDrafts(ds: Doc[]): Promise<Doc[]> {
  const ok = await Promise.all(ds.map((d) => keepDraft(d)));
  return ds.filter((_, i) => !ok[i]);
}

/** Replaces the text from outside the editor (a checkbox in preview, a restore). */
export function replaceText(d: Doc, text: string) {
  edit(d, text, "app");
}

function schedule(d: Doc, ms = SAVE_DELAY_MS) {
  window.clearTimeout(d.timer);
  d.timer = window.setTimeout(() => void save(d), ms);
}

/** Saves now. True once everything typed so far is on disk. */
export async function save(d: Doc): Promise<boolean> {
  window.clearTimeout(d.timer);
  if (d.saving) await d.saving;
  if (!d.dirty) return true;
  d.saving = write(d).finally(() => {
    d.saving = null;
  });
  await d.saving;
  return !d.dirty;
}

/**
 * Saves every open note. Returns the ones that still aren't saved (the
 * save failed, or the note is locked meanwhile); empty means all is on disk.
 */
export async function saveAll(): Promise<Doc[]> {
  for (let round = 0; round < 3; round++) {
    await Promise.all([...docs.values()].map((d) => save(d)));
    const left = [...docs.values()].filter((d) => d.dirty);
    // Typing during a save leaves a note dirty: go round again. A failed
    // save won't fix itself in a moment, so stop there.
    if (!left.length || left.every((d) => d.state === "error")) return left;
  }
  return [...docs.values()].filter((d) => d.dirty);
}

/** Ctrl+S: write even if nothing seems to have changed. */
export async function forceSave(d: Doc) {
  d.dirty = true;
  await save(d);
}

async function write(d: Doc): Promise<void> {
  for (let attempt = 0; attempt < 3; attempt++) {
    const text = d.text;
    setState(d, "saving");
    try {
      // Only overwrite the version we last saw; otherwise merge first.
      await api.writeNote(d.path, text, d.saved);
      d.saved = text;
      // Typing during the write keeps it dirty; the timer saves again.
      if (d.text === text) {
        d.dirty = false;
        setState(d, "saved");
        host.hideErrorBanner();
      } else {
        setState(d, "unsaved");
      }
      return;
    } catch (e) {
      if (String(e) === "CHANGED_ON_DISK") {
        const disk = await api.readNote(d.path).catch(() => null);
        if (disk === null) continue;
        await mergeFromDisk(d, disk);
        continue; // write the merged text
      }
      setState(d, "error");
      const s = String(e);
      if (s === "LOCKED" || s.startsWith("LOCKED_FOLDER:")) {
        // Locked meanwhile, or a new note in a locked folder: the text waits here.
        const folder = s.startsWith("LOCKED_FOLDER:") ? s.slice("LOCKED_FOLDER:".length) : null;
        const unlockAndSave = () =>
          void host.unlock(d.path, folder).then((ok) => {
            if (ok) void save(d);
          });
        host.banner(
          folder !== null
            ? `“${d.name}” is in a locked folder. Enter the folder's password to save it — your text is safe here meanwhile.`
            : `“${d.name}” is locked. Enter its password to save your changes.`,
          "error",
          [{ label: "Unlock and save", run: unlockAndSave }],
        );
        return;
      }
      host.banner(`Couldn't save “${d.name}”: ${e}`, "error", [{ label: "Try again", run: () => void save(d) }]);
      return;
    }
  }
}

/** The file changed on disk (another app, or sync). */
export async function onDiskChanged(d: Doc) {
  const disk = await api.readNote(d.path).catch(() => null);
  if (disk === null || disk === d.saved) return; // our own save, or gone
  if (!d.dirty) {
    d.saved = disk;
    d.text = disk;
    notify(d, "disk");
    host.toast(`“${d.name}” was updated from disk.`);
  } else {
    await mergeFromDisk(d, disk);
  }
}

/**
 * Another app changed the note while it had unsaved edits: merge their
 * version with ours (base = what we last saved) and save the result.
 */
async function mergeFromDisk(d: Doc, disk: string) {
  for (let attempt = 0; attempt < 3; attempt++) {
    const mine = d.text;
    const r = await api.mergeText(d.saved, mine, disk);
    if (d.text !== mine) continue; // typed meanwhile: merge again
    d.saved = disk;
    d.text = r.text;
    notify(d, "merge");
    d.dirty = r.text !== disk;
    if (d.dirty) {
      setState(d, "unsaved");
      schedule(d, 400);
    } else {
      setState(d, "saved");
    }
    if (r.conflicts > 0) {
      const places = r.conflicts === 1 ? "one place" : `${r.conflicts} places`;
      host.banner(
        `Another app changed “${d.name}” while you were typing. In ${places} you both edited the same lines, so both versions were kept, yours first. Check for repeated text.`,
        "warn",
      );
    } else {
      host.toast("Merged changes made in another app.");
    }
    return;
  }
}

/** The note was deleted outside Marrow: keep it, so typing saves it again. */
export function onDiskDeleted(d: Doc) {
  d.saved = "";
  d.dirty = true;
  setState(d, "unsaved");
  host.banner(`“${d.name}” was deleted or moved outside Marrow. Keep typing to save it again here.`, "warn");
}

/** A note (or a folder holding it) was renamed. */
export function moved(oldPath: string, newPath: string) {
  for (const d of [...docs.values()]) {
    let p: string | null = null;
    if (d.path === oldPath) p = newPath;
    else if (d.path.startsWith(oldPath + "/")) p = newPath + d.path.slice(oldPath.length);
    if (p) {
      docs.delete(d.path);
      d.path = p;
      docs.set(p, d);
    }
  }
}

/** Dropped without saving (the file went to the trash). */
export function forget(path: string) {
  for (const d of [...docs.values()]) {
    if (d.path === path || d.path.startsWith(path + "/")) {
      window.clearTimeout(d.timer);
      window.clearTimeout(d.draftTimer);
      d.dirty = false;
      docs.delete(d.path);
    }
  }
}

/** The worst state of all open docs, for the status bar. */
export function overallState(): SaveState {
  const s = [...docs.values()].map((d) => d.state);
  if (s.includes("error")) return "error";
  if (s.includes("saving")) return "saving";
  if (s.includes("unsaved")) return "unsaved";
  return "saved";
}

/** If a save of this note failed earlier, offer the kept text back. */
export async function offerBackup(d: Doc) {
  const b = await api.backupFor(d.path).catch(() => null);
  if (!b) return;
  if (b.text === d.saved) {
    void api.discardBackup(d.path);
    return;
  }
  const when = new Date(b.savedAt).toLocaleString(undefined, { dateStyle: "medium", timeStyle: "short" });
  host.banner(`Text you wrote in “${d.name}” on ${when} wasn't saved (Marrow closed, or the save failed), so it was kept.`, "warn", [
    {
      label: "Restore it",
      run: () => {
        replaceText(d, b.text);
        host.toast("Restored. It will save in a moment.");
      },
    },
    { label: "Discard", run: () => void api.discardBackup(d.path) },
  ]);
}
