// The Files view. Top-level folders are small-caps groups with a count;
// deeper folders indent beneath them. Notes and canvases open on click;
// everything can be renamed (F2), trashed (Delete) or dragged to a folder.

import { displayName, isCanvasPath, isNotePath, parentOf, type TreeNode } from "./api";
import { isLockedFolder, isLockedNote, lockedFolderOf } from "./lock";
import { showMenu, type MenuItem } from "./menu";
import { h, icon, storage, store } from "./util";

export interface TreeApp {
  activePath(): string | null;
  open(path: string, how: "replace" | "tab" | "split"): void;
  rename(path: string, newName: string): Promise<boolean>;
  move(path: string, folder: string): void;
  trash(path: string, isFolder: boolean): void;
  newNote(folder: string): void;
  newFolder(folder: string): void;
  newCanvas(folder: string): void;
  /** A note showing a folder's notes as a database. */
  newDatabase(folder: string): void;
  reveal(path: string): void;
  dailyFolder(): string;
  /** Prints the note, or saves it as PDF or HTML. */
  exportNote(path: string, as: "pdf" | "html" | "print"): void;
  /** Starred notes and canvases, in the order they were starred. */
  starred(): string[];
  toggleStar(path: string): void;
  /** Earlier versions of a note. */
  history(path: string): void;
  /** Import from Notion, Evernote, OneNote… */
  importNotes(): void;
  /** Lock a note or folder with a password, or remove its lock. */
  lock(path: string, isFolder: boolean): void;
  unlockRemove(path: string, isFolder: boolean): void;
  /** Publish a folder ("" = the vault) as a website. */
  publish(folder: string): void;
}

const lockIcon = () => icon("lock-simple", "font-size:13px;flex:none;opacity:.65;margin-left:auto");

export const DRAG_TYPE = "text/x-marrow-path";

const isOpenable = (p: string) => isNotePath(p) || isCanvasPath(p);

export class Tree {
  data: TreeNode | null = null;
  /** Explicit open/closed per folder; groups default open, nested folders closed. */
  private open: Record<string, boolean> = {};
  private key = "";

  constructor(
    private el: HTMLElement,
    private app: TreeApp,
  ) {
    this.makeDropTarget(el, "");
    el.addEventListener("contextmenu", (e) => {
      if ((e.target as HTMLElement).closest(".tree-row, .tree-head")) return;
      e.preventDefault();
      showMenu(e.clientX, e.clientY, [
        { label: "New note", icon: "note-pencil", run: () => app.newNote("") },
        { label: "New canvas", icon: "squares-four", run: () => app.newCanvas("") },
        { label: "New folder", icon: "folder-simple-plus", run: () => app.newFolder("") },
        "-",
        { label: "Import notes…", icon: "tray-arrow-down", run: () => app.importNotes() },
        { label: "Publish as a website…", icon: "globe", run: () => app.publish("") },
      ]);
    });
  }

  setVault(root: string) {
    this.key = `marrow.folders.${root}`;
    this.open = storage<Record<string, boolean>>(this.key, {});
  }

  set(data: TreeNode) {
    this.data = data;
    this.render();
  }

  private isOpen(path: string, depth: number) {
    return this.open[path] ?? depth === 0;
  }

  private setOpen(path: string, on: boolean) {
    this.open[path] = on;
    store(this.key, this.open);
  }

  /** Every .md and .canvas, for counts, the palette and daily notes. */
  files(): string[] {
    const out: string[] = [];
    const walk = (n: TreeNode) => {
      if (n.kind === "File") out.push(n.path);
      else n.children.forEach(walk);
    };
    if (this.data) walk(this.data);
    return out;
  }

  folders(): string[] {
    const out: string[] = [];
    const walk = (n: TreeNode) => {
      if (n.kind === "Folder") {
        if (n.path) out.push(n.path);
        n.children.forEach(walk);
      }
    };
    if (this.data) walk(this.data);
    return out;
  }

  render() {
    const root = this.el;
    root.replaceChildren();
    if (!this.data) return;
    const kids = this.data.children;
    if (kids.length === 0) {
      root.append(h("p.tree-empty", null, "This vault is empty. Create a note to start."));
      return;
    }
    const folders = kids.filter((k) => k.kind === "Folder");
    const files = this.sorted(kids.filter((k) => k.kind === "File"), "");
    const starred = this.app.starred().filter((p) => this.find(p));
    if (starred.length) root.append(this.starredGroup(starred));
    for (const f of folders) root.append(this.group(f));
    if (files.length) {
      const list = h("div.tree-items", { style: folders.length ? "margin-top:6px" : "" });
      for (const f of files) list.append(this.fileRow(f, 0, true));
      root.append(list);
    }
    this.markActive();
  }

  private sorted(nodes: TreeNode[], folder: string): TreeNode[] {
    const daily = folder !== "" && folder === this.app.dailyFolder();
    const coll = new Intl.Collator(undefined, { numeric: true, sensitivity: "base" });
    return [...nodes].sort((a, b) => {
      if (a.kind !== b.kind) return a.kind === "Folder" ? -1 : 1;
      const c = coll.compare(displayName(a.path), displayName(b.path));
      return daily && a.kind === "File" ? -c : c;
    });
  }

  private count(n: TreeNode): number {
    if (n.kind === "File") return isOpenable(n.path) ? 1 : 0;
    return n.children.reduce((s, c) => s + this.count(c), 0);
  }

  /** Starred notes, above the folders. */
  private starredGroup(paths: string[]): HTMLElement {
    const key = "\u0000starred";
    const open = this.isOpen(key, 0);
    const head = h(
      "button.tree-head.is-starred",
      { "aria-expanded": String(open), title: "Starred notes" },
      icon(open ? "caret-down" : "caret-right"),
      h("span.name", null, "Starred"),
      h("span.count", null, String(paths.length)),
    );
    head.addEventListener("click", () => {
      this.setOpen(key, !open);
      this.render();
    });
    const box = h("div.tree-group.tree-starred", null, head);
    if (open) {
      const items = h("div.tree-items");
      for (const p of paths) {
        const row = this.fileRow(this.find(p)!, 0, false);
        row.prepend(icon("star", "font-size:14px;flex:none;color:var(--color-process-yellow)"));
        items.append(row);
      }
      box.append(items);
    }
    return box;
  }

  private group(f: TreeNode): HTMLElement {
    const open = this.isOpen(f.path, 0);
    const head = h(
      "button.tree-head",
      { "aria-expanded": String(open), dataset: { path: f.path }, title: f.path },
      icon(open ? "caret-down" : "caret-right"),
      h("span.name", null, f.name),
      isLockedFolder(f.path) ? icon("lock-simple", "font-size:13px;flex:none;opacity:.65") : null,
      h("span.count", null, String(this.count(f))),
    );
    head.addEventListener("click", () => {
      this.setOpen(f.path, !open);
      this.render();
    });
    this.bindRow(head, f, true);
    const box = h("div.tree-group", null, head);
    if (open) {
      const items = h("div.tree-items");
      for (const c of this.sorted(f.children, f.path)) items.append(...this.node(c, 0));
      box.append(items);
    }
    return box;
  }

  private node(n: TreeNode, depth: number): HTMLElement[] {
    if (n.kind === "File") return [this.fileRow(n, depth, false)];
    const open = this.isOpen(n.path, depth + 1);
    const row = h(
      "button.tree-row.is-folder",
      { "aria-expanded": String(open), dataset: { path: n.path }, style: `--depth:${depth}`, title: n.path },
      h("i", { class: `ph-duotone ph-${open ? "caret-down" : "caret-right"} caret` }),
      icon(open ? "folder-simple-dashed" : "folder-simple", "font-size:15px;flex:none;opacity:.7"),
      h("span", null, n.name),
      isLockedFolder(n.path) ? lockIcon() : null,
    );
    row.addEventListener("click", () => {
      this.setOpen(n.path, !open);
      this.render();
    });
    this.bindRow(row, n, true);
    const out: HTMLElement[] = [row];
    if (open) for (const c of this.sorted(n.children, n.path)) out.push(...this.node(c, depth + 1));
    return out;
  }

  private fileRow(n: TreeNode, depth: number, atRoot: boolean): HTMLElement {
    const note = isNotePath(n.path);
    const canvas = isCanvasPath(n.path);
    const cls = note ? "" : canvas ? "" : ".is-file";
    const row = h(
      `button.tree-row${cls}${atRoot ? ".is-root" : ""}` as "button",
      { dataset: { path: n.path }, style: `--depth:${depth}`, title: n.path },
      canvas ? icon("squares-four", "font-size:15px;flex:none") : null,
      !note && !canvas ? icon(/\.(png|jpe?g|gif|webp|svg|bmp|avif)$/i.test(n.name) ? "image" : "file", "font-size:15px;flex:none") : null,
      h("span", null, note || canvas ? displayName(n.path) : n.name),
    );
    for (const i of row.querySelectorAll("i")) i.classList.add("kind");
    if (note && isLockedNote(n.path)) row.append(lockIcon());
    if (note || canvas) {
      row.addEventListener("click", (e) => this.app.open(n.path, e.ctrlKey || e.metaKey ? "tab" : "replace"));
      row.addEventListener("auxclick", (e) => {
        if (e.button === 1) this.app.open(n.path, "tab");
      });
    }
    this.bindRow(row, n, false);
    return row;
  }

  private bindRow(row: HTMLElement, n: TreeNode, isFolder: boolean) {
    row.addEventListener("contextmenu", (e) => {
      e.preventDefault();
      e.stopPropagation();
      showMenu(e.clientX, e.clientY, this.menuFor(n, isFolder, row));
    });
    row.addEventListener("keydown", (e) => {
      if (e.key === "Delete") {
        e.preventDefault();
        this.app.trash(n.path, isFolder);
      } else if (e.key === "F2") {
        e.preventDefault();
        this.startRename(row, n);
      }
    });
    row.draggable = true;
    row.addEventListener("dragstart", (e) => {
      e.dataTransfer?.setData(DRAG_TYPE, n.path);
      e.dataTransfer!.effectAllowed = "copyMove";
      row.classList.add("is-dragging");
    });
    row.addEventListener("dragend", () => row.classList.remove("is-dragging"));
    if (isFolder) this.makeDropTarget(row, n.path);
  }

  private menuFor(n: TreeNode, isFolder: boolean, row: HTMLElement): MenuItem[] {
    const folder = isFolder ? n.path : parentOf(n.path);
    const items: MenuItem[] = [];
    if (!isFolder && isOpenable(n.path)) {
      items.push({ label: "Open in new tab", icon: "plus-square", run: () => this.app.open(n.path, "tab") });
      if (isNotePath(n.path)) items.push({ label: "Open in split pane", icon: "columns", run: () => this.app.open(n.path, "split") });
      const starred = this.app.starred().includes(n.path);
      items.push({ label: starred ? "Unstar" : "Star", icon: "star", run: () => this.app.toggleStar(n.path) });
      items.push("-");
      if (isNotePath(n.path)) {
        items.push(
          { label: "Export to PDF…", icon: "file-pdf", run: () => this.app.exportNote(n.path, "pdf") },
          { label: "Export to HTML…", icon: "file-html", run: () => this.app.exportNote(n.path, "html") },
          { label: "Print…", icon: "printer", run: () => this.app.exportNote(n.path, "print") },
          "-",
          { label: "Version history…", icon: "clock-counter-clockwise", run: () => this.app.history(n.path) },
          isLockedNote(n.path)
            ? { label: "Remove lock…", icon: "lock-simple-open", run: () => this.app.unlockRemove(n.path, false) }
            : { label: "Lock with password…", icon: "lock-simple", run: () => this.app.lock(n.path, false) },
          "-",
        );
      }
    }
    items.push(
      { label: "New note here", icon: "note-pencil", run: () => this.app.newNote(folder) },
      { label: "New canvas here", icon: "squares-four", run: () => this.app.newCanvas(folder) },
      { label: "New folder here", icon: "folder-simple-plus", run: () => this.app.newFolder(folder) },
      ...(isFolder ? [{ label: "View as database", icon: "database", run: () => this.app.newDatabase(folder) }] : []),
      ...this.lockItems(n, isFolder),
      ...(isFolder ? [{ label: "Publish as a website…", icon: "globe", run: () => this.app.publish(n.path) }] : []),
      "-",
      { label: "Rename", icon: "pencil-simple", run: () => this.startRename(row, n) },
      { label: isFolder ? "Show in file manager" : "Show in file manager", icon: "arrow-square-out", run: () => this.app.reveal(n.path) },
      { label: "Move to trash", icon: "trash", danger: true, run: () => this.app.trash(n.path, isFolder) },
    );
    return items;
  }

  private lockItems(n: TreeNode, isFolder: boolean): MenuItem[] {
    if (!isFolder) return [];
    if (isLockedFolder(n.path)) return [{ label: "Remove folder lock…", icon: "lock-simple-open", run: () => this.app.unlockRemove(n.path, true) }];
    if (lockedFolderOf(n.path) !== null) return [];
    return [{ label: "Lock folder with password…", icon: "lock-simple", run: () => this.app.lock(n.path, true) }];
  }

  /** Swaps a row for a text box. */
  startRename(row: HTMLElement | null, n?: TreeNode) {
    if (!row || !n) return;
    const input = h("input.tree-rename", {
      value: n.kind === "Folder" ? n.name : isOpenable(n.path) ? displayName(n.path) : n.name,
      spellcheck: false,
      style: row.getAttribute("style") ?? "",
    });
    row.replaceWith(input);
    input.focus();
    input.select();
    let done = false;
    const finish = async (commit: boolean) => {
      if (done) return;
      done = true;
      if (commit) await this.app.rename(n.path, input.value);
      this.render();
    };
    input.addEventListener("keydown", (e) => {
      if (e.key === "Enter") void finish(true);
      else if (e.key === "Escape") void finish(false);
      e.stopPropagation();
    });
    input.addEventListener("blur", () => void finish(true));
  }

  renameByPath(path: string) {
    const row = this.el.querySelector<HTMLElement>(`[data-path="${CSS.escape(path)}"]`);
    const n = this.find(path);
    if (row && n) this.startRename(row, n);
  }

  find(path: string): TreeNode | null {
    const walk = (n: TreeNode): TreeNode | null => {
      if (n.path === path) return n;
      for (const c of n.children) {
        const f = walk(c);
        if (f) return f;
      }
      return null;
    };
    return this.data ? walk(this.data) : null;
  }

  private makeDropTarget(el: HTMLElement, folder: string) {
    el.addEventListener("dragover", (e) => {
      if (!e.dataTransfer?.types.includes(DRAG_TYPE)) return;
      e.preventDefault();
      e.stopPropagation();
      e.dataTransfer.dropEffect = "move";
      el.classList.add("is-drop");
    });
    el.addEventListener("dragleave", () => el.classList.remove("is-drop"));
    el.addEventListener("drop", (e) => {
      el.classList.remove("is-drop");
      const path = e.dataTransfer?.getData(DRAG_TYPE);
      if (!path) return;
      e.preventDefault();
      e.stopPropagation();
      if (folder === path || folder.startsWith(path + "/")) return;
      this.app.move(path, folder);
    });
  }

  markActive() {
    const active = this.app.activePath();
    for (const el of this.el.querySelectorAll<HTMLElement>(".tree-row")) {
      el.classList.toggle("is-active", el.dataset.path === active);
    }
  }

  /** Opens the folders above `path` and scrolls to it. */
  reveal(path: string) {
    let changed = false;
    const parts = parentOf(path).split("/").filter(Boolean);
    for (let i = 0; i < parts.length; i++) {
      const p = parts.slice(0, i + 1).join("/");
      if (!this.isOpen(p, i)) {
        this.open[p] = true;
        changed = true;
      }
    }
    if (changed) {
      store(this.key, this.open);
      this.render();
    } else this.markActive();
    const rows = [...this.el.querySelectorAll<HTMLElement>(`.tree-row[data-path="${CSS.escape(path)}"]`)];
    (rows.find((r) => !r.closest(".tree-starred")) ?? rows[0])?.scrollIntoView({ block: "nearest" });
  }

  /** Keeps renamed folders open. */
  moved(oldPath: string, newPath: string) {
    for (const k of Object.keys(this.open)) {
      if (k === oldPath || k.startsWith(oldPath + "/")) {
        this.open[newPath + k.slice(oldPath.length)] = this.open[k];
        delete this.open[k];
      }
    }
    store(this.key, this.open);
  }
}
