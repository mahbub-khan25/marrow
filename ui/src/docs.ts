// Open notes and their saving.
//
// A `Doc` is one note open somewhere: in a tab, in the split pane, or both.
// Every pane showing it reads and writes `doc.text`; the doc saves itself a
// moment after the last edit, refuses to overwrite a version it hasn't seen
// (merging instead), and keeps a backup when a save fails.

import { api, displayName } from "./api";

/** Save this long after the last keystroke. */
const SAVE_DELAY_MS = 2000;

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
}

/** Replaces the text from outside the editor (a checkbox in preview, a restore). */
export function replaceText(d: Doc, text: string) {
  edit(d, text, "app");
}

function schedule(d: Doc, ms = SAVE_DELAY_MS) {
  window.clearTimeout(d.timer);
  d.timer = window.setTimeout(() => void save(d), ms);
}

export async function save(d: Doc): Promise<void> {
  window.clearTimeout(d.timer);
  if (d.saving) await d.saving;
  if (!d.dirty) return;
  d.saving = write(d).finally(() => {
    d.saving = null;
  });
  await d.saving;
}

export async function saveAll() {
  await Promise.all([...docs.values()].map((d) => save(d)));
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
  host.banner(`Text you wrote in “${d.name}” on ${when} couldn't be saved at the time and was kept.`, "warn", [
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
