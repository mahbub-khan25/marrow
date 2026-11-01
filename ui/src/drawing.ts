// Drawings inside notes. A drawing is a small canvas (a `.canvas` file in
// the attachments folder) shown where its embed sits in the note:
//
//   ![[Drawing 2026-10-31 174312.canvas|360]]
//
// The number is its height. It's drawn on right there, in Preview and in
// Source, with the canvas tools (pen, shapes, arrows, text, colours). The
// grip at the bottom makes it taller or shorter; "Open full size" opens the
// same file as a canvas tab.

import { api, displayName } from "./api";
import { ATTACHMENT_FOLDER } from "./attach";
import { CanvasView, type CanvasApp } from "./canvas";
import { confirmDialog } from "./lock";
import { h, icon, isoDay } from "./util";

export const DRAWING_HEIGHT = 360;
const MIN = 120;
const MAX = 2000;
const EMPTY = '{\n\t"nodes": [],\n\t"edges": []\n}';

export interface DrawingRef {
  /** The link as written: `Drawing 2026-10-31 174312.canvas`. */
  target: string;
  height: number;
}

const LINE = /^\s*!\[\[([^\]|]+?\.canvas)\s*(?:\|\s*(\d+)\s*)?\]\]\s*$/i;

/** The drawing on a line that is only a drawing's embed. */
export function drawingInLine(line: string): DrawingRef | null {
  const m = LINE.exec(line);
  if (!m) return null;
  const n = Number(m[2]);
  return { target: m[1].trim(), height: m[2] ? clampHeight(n) : DRAWING_HEIGHT };
}

export const clampHeight = (n: number) => Math.max(MIN, Math.min(MAX, Math.round(n) || DRAWING_HEIGHT));

/** The embed line for a drawing at `path`. */
export const drawingEmbed = (path: string, height = DRAWING_HEIGHT) => `![[${path.split("/").pop()}|${clampHeight(height)}]]`;

/** The same line with a new height. */
export function lineWithHeight(line: string, height: number): string {
  const d = drawingInLine(line);
  if (!d) return line;
  const indent = /^\s*/.exec(line)?.[0] ?? "";
  return `${indent}![[${d.target}|${clampHeight(height)}]]`;
}

/** Drawings just made: they open ready for the pen. */
const fresh = new Set<string>();

/** Makes an empty drawing in the attachments folder; returns its path. */
export async function createDrawing(): Promise<string> {
  const now = new Date();
  const p = (n: number) => String(n).padStart(2, "0");
  const stamp = `${isoDay(now)} ${p(now.getHours())}${p(now.getMinutes())}${p(now.getSeconds())}`;
  const path = await api.createFile(ATTACHMENT_FOLDER, `Drawing ${stamp}`, "canvas", EMPTY);
  fresh.add(path);
  return path;
}

export interface DrawingHost extends CanvasApp {
  /** Finds what a link points to, from the note it's in. */
  resolve(target: string): Promise<string | null>;
}

export interface MountedDrawing {
  el: HTMLElement;
  /** Saves and lets go of it. */
  destroy(): void;
}

const mounted = new Set<{ view: CanvasView }>();

/** Saves every drawing on screen (before closing, switching vaults…). */
export async function flushDrawings(): Promise<void> {
  await Promise.all([...mounted].map((m) => m.view.flush()));
}

/** A file changed: drawings showing it catch up (sync, another window). */
export function drawingsChanged() {
  for (const m of mounted) void m.view.onDiskChanged();
}

/**
 * Draws a drawing into `box` (which takes its height), ready to draw on.
 * `onHeight` saves a new height (dragged with the grip at the bottom);
 * `onRemove` takes its embed out of the note (the trash button, after
 * asking), and the drawing's file then goes to the trash.
 */
export function mountDrawing(box: HTMLElement, ref: DrawingRef, host: DrawingHost, onHeight?: (h: number) => void, onRemove?: () => void): MountedDrawing {
  box.classList.add("drawing");
  box.contentEditable = "false";
  box.style.height = `${ref.height}px`;
  const board = h("div.drawing-board.canvas-view");
  const grip = h("div.drawing-grip", { title: "Drag to make the drawing taller or shorter" });
  const open = h("button.drawing-open", { type: "button", title: "Open full size" }, icon("arrows-out-simple"));
  const del = h("button.drawing-delete", { type: "button", title: "Delete drawing" }, icon("trash"));
  box.replaceChildren(board, open, grip);
  if (onRemove) box.append(del);

  const view = new CanvasView(board, host, { inline: true });
  const entry = { view };
  mounted.add(entry);
  let path: string | null = null;
  let gone = false;

  void (async () => {
    path = await host.resolve(ref.target).catch(() => null);
    if (gone) return;
    if (!path) {
      box.replaceChildren(h("div.drawing-missing", null, icon("pencil-simple-slash"), ` Missing drawing: ${displayName(ref.target)}`));
      return;
    }
    try {
      await view.load(path);
    } catch (e) {
      host.toast(`Couldn't open the drawing: ${e}`, "error");
      return;
    }
    if (fresh.delete(path)) view.startDrawing();
  })();

  open.addEventListener("click", (e) => {
    e.stopPropagation();
    if (path) void view.flush().then(() => host.openPath(path!, false));
  });

  del.addEventListener("click", (e) => {
    e.stopPropagation();
    void (async () => {
      if (!(await confirmDialog("Delete this drawing?", "It's taken out of the note, and its file goes to Marrow's trash, where you can restore it.", "Delete"))) return;
      const file = path;
      gone = true;
      mounted.delete(entry);
      await view.flush().catch(() => {});
      onRemove?.();
      if (file) await api.trash(file).catch((err) => host.toast(`Couldn't move the drawing's file to the trash: ${err}`, "error"));
    })();
  });

  // The note mustn't react to what happens in the drawing: no paragraph
  // editing on double-click, no text selection, no note shortcuts.
  for (const type of ["pointerdown", "mousedown", "mouseup", "click", "dblclick", "keydown", "keyup", "keypress", "input", "beforeinput", "paste", "copy", "cut", "dragstart"]) {
    box.addEventListener(type, (e) => e.stopPropagation());
  }

  grip.addEventListener("pointerdown", (e) => {
    e.preventDefault();
    e.stopPropagation();
    const y0 = e.clientY;
    const h0 = box.getBoundingClientRect().height;
    grip.setPointerCapture(e.pointerId);
    const move = (ev: PointerEvent) => (box.style.height = `${clampHeight(h0 + ev.clientY - y0)}px`);
    const up = () => {
      grip.removeEventListener("pointermove", move);
      grip.removeEventListener("pointerup", up);
      grip.removeEventListener("pointercancel", up);
      const now = clampHeight(box.getBoundingClientRect().height);
      if (now !== ref.height) onHeight?.(now);
    };
    grip.addEventListener("pointermove", move);
    grip.addEventListener("pointerup", up);
    grip.addEventListener("pointercancel", up);
  });

  return {
    el: box,
    destroy() {
      if (gone) return;
      gone = true;
      mounted.delete(entry);
      void view.flush();
    },
  };
}
