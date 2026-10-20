// Password-locked notes and folders: the dialogs, which paths are locked
// (for icons), and locking everything again after a while away.
//
// The encryption itself happens in Rust; the window only ever sees plain
// text. Reading a locked note before its password is entered fails with
// "LOCKED" (or "LOCKED_FOLDER:<folder>" when saving a note in a locked
// folder), and callers ask for the password with `unlockPath`, then retry.

import { api, displayName, onLocksChanged } from "./api";
import { prefs } from "./prefs";
import { h, icon } from "./util";

export function isLockedError(e: unknown): boolean {
  const s = String(e);
  return s === "LOCKED" || s.startsWith("LOCKED_FOLDER:");
}

/** The folder named in a LOCKED_FOLDER error. */
export function lockedFolderIn(e: unknown): string | null {
  const s = String(e);
  return s.startsWith("LOCKED_FOLDER:") ? s.slice("LOCKED_FOLDER:".length) : null;
}

// ------------------------------------------------------- what is locked

let notes = new Set<string>();
let folders = new Set<string>();
const listeners = new Set<() => void>();

export function onLockList(fn: () => void) {
  listeners.add(fn);
}

export async function refreshLocked() {
  const r = await api.lockedPaths().catch(() => null);
  if (!r) return;
  const same = r.notes.length === notes.size && r.folders.length === folders.size && r.notes.every((n) => notes.has(n)) && r.folders.every((f) => folders.has(f));
  if (same) return;
  notes = new Set(r.notes);
  folders = new Set(r.folders);
  for (const fn of listeners) fn();
}

export const isLockedNote = (path: string) => notes.has(path);
export const isLockedFolder = (path: string) => folders.has(path);

/** The locked folder holding `path`, if any. */
export function lockedFolderOf(path: string): string | null {
  let best: string | null = null;
  for (const f of folders) {
    if ((f === "" || path === f || path.startsWith(f + "/")) && (best === null || f.length > best.length)) best = f;
  }
  return best;
}

/** Locked notes, and notes in locked folders, need the password to read. */
export const needsPassword = (path: string) => isLockedNote(path) || lockedFolderOf(path) !== null;

// -------------------------------------------------------------- dialogs

interface PasswordDialog {
  title: string;
  lede: string;
  hint?: string | null;
  /** Ask twice and offer a hint (when setting a password). */
  setting: boolean;
  button: string;
  busy: string;
  /** Returns an error message to show, or nothing when done. */
  run: (password: string, hint: string | null) => Promise<string | void>;
}

function passwordDialog(o: PasswordDialog): Promise<boolean> {
  return new Promise((resolve) => {
    const pw = h("input.input", { type: "password", autocomplete: "new-password", placeholder: "Password", spellcheck: false }) as HTMLInputElement;
    const again = h("input.input", { type: "password", autocomplete: "new-password", placeholder: "The same password again", spellcheck: false }) as HTMLInputElement;
    const hint = h("input.input", { placeholder: "Optional — something that reminds you of it", spellcheck: false, maxlength: 120 }) as HTMLInputElement;
    const show = h("button.lock-show", { type: "button", tabindex: -1, title: "Show the password", onclick: () => toggleShow() }, icon("eye"));
    const error = h("div.dlg-error");
    const go = h("button.btn.btn-primary", { type: "submit" }, icon(o.setting ? "lock-simple" : "lock-simple-open"), o.button) as HTMLButtonElement;
    const cancel = h("button.btn.btn-secondary", { type: "button", onclick: () => close(false) }, "Cancel");
    const form = h(
      "form.dlg.lock-dlg",
      { role: "dialog", "aria-label": o.title },
      h("h2", null, o.title),
      h("p.dlg-lede", null, o.lede),
      o.hint ? h("p.lock-hint", null, icon("lightbulb"), " Hint: ", o.hint) : null,
      h("div.field", null, h("label", null, "Password"), h("div.lock-pw", null, pw, show)),
      o.setting ? h("div.field", null, h("label", null, "Confirm"), again) : null,
      o.setting ? h("div.field", null, h("label", null, "Hint"), hint) : null,
      o.setting
        ? h(
            "p.lock-warn",
            null,
            icon("warning"),
            h("span", null, "If you forget the password, the text can't be recovered — not by Marrow, not by anyone. Keep it somewhere safe."),
          )
        : null,
      error,
      h("div.dlg-foot", null, cancel, go),
    ) as HTMLFormElement;
    const toggleShow = () => {
      const t = pw.type === "password" ? "text" : "password";
      pw.type = again.type = t;
      show.replaceChildren(icon(t === "password" ? "eye" : "eye-slash"));
      pw.focus();
    };
    form.addEventListener("submit", (e) => {
      e.preventDefault();
      void submit();
    });
    const wrap = h("div.backdrop.dlg-wrap", null, form);
    document.body.append(wrap);
    document.documentElement.toggleAttribute("data-modal", true);
    pw.focus();
    const keys = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        e.stopPropagation();
        close(false);
      }
    };
    document.addEventListener("keydown", keys, true);

    let busy = false;
    async function submit() {
      if (busy) return;
      error.textContent = "";
      if (!pw.value) return pw.focus();
      if (o.setting) {
        if ([...pw.value].length < 6) {
          error.textContent = "Use at least 6 characters (a short sentence is easy to remember and hard to guess).";
          return pw.focus();
        }
        if (pw.value !== again.value) {
          error.textContent = "The two passwords aren't the same.";
          return again.focus();
        }
        if (hint.value && hint.value.trim().toLowerCase() === pw.value.trim().toLowerCase()) {
          error.textContent = "The hint can't be the password itself.";
          return hint.focus();
        }
      }
      busy = true;
      go.disabled = true;
      go.replaceChildren(icon("hourglass"), o.busy);
      const why = await o.run(pw.value, hint.value.trim() || null).catch((e) => String(e));
      busy = false;
      go.disabled = false;
      go.replaceChildren(icon(o.setting ? "lock-simple" : "lock-simple-open"), o.button);
      if (why) {
        error.textContent = why;
        pw.select();
        return;
      }
      close(true);
    }

    function close(ok: boolean) {
      pw.value = again.value = "";
      wrap.remove();
      document.removeEventListener("keydown", keys, true);
      document.documentElement.toggleAttribute("data-modal", false);
      resolve(ok);
    }
  });
}

/** A yes/no question in Marrow's own dialog. */
export function confirmDialog(title: string, lede: string, yes: string): Promise<boolean> {
  return new Promise((resolve) => {
    const ok = h("button.btn.btn-primary", { type: "button", onclick: () => close(true) }, yes);
    const card = h(
      "div.dlg",
      { role: "dialog", "aria-label": title },
      h("h2", null, title),
      h("p.dlg-lede", null, lede),
      h("div.dlg-foot", null, h("button.btn.btn-secondary", { type: "button", onclick: () => close(false) }, "Cancel"), ok),
    );
    const wrap = h("div.backdrop.dlg-wrap", null, card);
    document.body.append(wrap);
    document.documentElement.toggleAttribute("data-modal", true);
    ok.focus();
    const keys = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        e.stopPropagation();
        close(false);
      }
    };
    document.addEventListener("keydown", keys, true);
    function close(v: boolean) {
      wrap.remove();
      document.removeEventListener("keydown", keys, true);
      document.documentElement.toggleAttribute("data-modal", false);
      resolve(v);
    }
  });
}

const nameOf = (path: string) => (path === "" ? "the vault" : `“${displayName(path)}”`);

/** Asks for the password of a locked note (or of the locked folder it's in). */
export async function unlockPath(path: string, folderHint?: string | null): Promise<boolean> {
  const info = await api.lockInfo(path).catch(() => null);
  if (!info) return false;
  if (info.unlocked) return true;
  const folder = folderHint ?? info.folder;
  const target = info.locked ? path : (folder ?? path);
  const lede = info.locked
    ? `${nameOf(path)} is locked with a password. Enter it to read and edit it. It stays open until you lock everything again or close Marrow.`
    : folder === path
      ? `${nameOf(path)} is a locked folder. Enter its password to put notes in it; they're locked with it.`
      : `${nameOf(path)} is in the locked folder ${nameOf(folder ?? "")}. Enter the folder's password to save it there.`;
  const ok = await passwordDialog({
    title: info.locked ? "Unlock note" : "Unlock folder",
    lede,
    hint: info.hint,
    setting: false,
    button: "Unlock",
    busy: "Checking…",
    run: async (pw) => {
      try {
        await api.unlock(target, pw);
      } catch (e) {
        return String(e);
      }
    },
  });
  if (ok) armIdleLock();
  return ok;
}

/** Locks a note with a new password. `prepare` saves it first. */
export async function lockNoteFlow(path: string, prepare: () => Promise<void>): Promise<boolean> {
  const folder = lockedFolderOf(path);
  if (folder !== null) {
    const ok = await unlockPath(path, folder);
    if (!ok) return false;
    await prepare();
    return true; // saving it seals it with the folder's password
  }
  const ok = await passwordDialog({
    title: "Lock note",
    lede: `${nameOf(path)} will be encrypted on disk. Only someone with the password can read it, in Marrow or anywhere else. Its name stays visible. Links in it are hidden from the graph and backlinks, and its text from search.`,
    setting: true,
    button: "Lock note",
    busy: "Locking…",
    run: async (pw, hint) => {
      await prepare();
      await api.lockNote(path, pw, hint);
    },
  });
  if (ok) {
    await refreshLocked();
    armIdleLock();
  }
  return ok;
}

/** Locks every note in a folder with one password. */
export async function lockFolderFlow(path: string, prepare: () => Promise<void>): Promise<number | null> {
  let count: number | null = null;
  const ok = await passwordDialog({
    title: "Lock folder",
    lede: `Every note in ${nameOf(path)} and its subfolders will be encrypted with this password, and so will notes you add there later. Attachments (images, PDFs) aren't encrypted.`,
    setting: true,
    button: "Lock folder",
    busy: "Locking…",
    run: async (pw, hint) => {
      await prepare();
      count = await api.lockFolder(path, pw, hint);
    },
  });
  if (!ok) return null;
  await refreshLocked();
  armIdleLock();
  return count;
}

/** Turns a locked note or folder back into plain text. */
export async function removeLockFlow(path: string, isFolder: boolean): Promise<number | null> {
  const yes = await confirmDialog(
    isFolder ? "Remove folder lock" : "Remove lock",
    isFolder
      ? `The notes in ${nameOf(path)} will be saved as plain text again, readable by anyone with access to the files.`
      : `${nameOf(path)} will be saved as plain text again, readable by anyone with access to the file.`,
    "Remove lock",
  );
  if (!yes) return null;
  for (let attempt = 0; attempt < 2; attempt++) {
    try {
      const n = await api.removeLock(path);
      await refreshLocked();
      return n;
    } catch (e) {
      if (!isLockedError(e) || attempt > 0) throw e;
      if (!(await unlockPath(path, lockedFolderIn(e)))) return null;
    }
  }
  return null;
}

// ------------------------------------------------- locking again later

let idleTimer: number | undefined;
let lockAll: () => Promise<void> = async () => {};

/** What "lock everything now" does (main.ts saves and closes notes first). */
export function setLockAll(fn: () => Promise<void>) {
  lockAll = fn;
}

/** Restarts the countdown to locking again (after unlocking, and on activity). */
export function armIdleLock() {
  window.clearTimeout(idleTimer);
  const min = prefs.lockAfter;
  if (!min) {
    idleTimer = undefined;
    return;
  }
  idleTimer = window.setTimeout(() => void lockAll(), min * 60_000);
}

let armed = false;
/** Watches for activity, and refreshes lock icons when something changes. */
export function initLocks() {
  if (armed) return;
  armed = true;
  let last = 0;
  const poke = () => {
    // Activity only matters while a timer is running; don't reset it constantly.
    if (idleTimer === undefined) return;
    const now = Date.now();
    if (now - last < 5000) return;
    last = now;
    armIdleLock();
  };
  for (const ev of ["keydown", "pointerdown", "wheel"]) window.addEventListener(ev, poke, { passive: true, capture: true });
  void onLocksChanged(() => void refreshLocked());
}

export function stopIdleLock() {
  window.clearTimeout(idleTimer);
  idleTimer = undefined;
}
