// Canvas: a board of cards saved as a `.canvas` file in the JSON Canvas
// format (the one Obsidian uses), so boards open in either app.
//
// Drag a card to move it, drag its right-hand dot onto another card to
// connect them, drag the corner to resize. Drag the background (or scroll)
// to pan, Ctrl+scroll to zoom. Double-click the background for a text card,
// double-click a note card to open the note. Drop notes from the sidebar.
//
// The toolbar also draws: rectangles, ellipses and diamonds (with a label),
// arrows, and freehand pen strokes, in six colours. They're stored as text
// cards with extra fields (`shape`, `ink`), so Obsidian still opens the file:
// it shows a shape as a text card with its label, and a drawing as a card
// saying it was drawn in Marrow.

import { convertFileSrc } from "@tauri-apps/api/core";
import { ask } from "@tauri-apps/plugin-dialog";
import { openUrl } from "@tauri-apps/plugin-opener";

import { api, displayName, parentOf } from "./api";
import { DRAG_TYPE } from "./tree";
import { FILLS, placePath, shapeDef, SHAPES } from "./shapes";
import { h, icon, plainText, trunc } from "./util";

type Side = "top" | "right" | "bottom" | "left";

export interface CNode {
  id: string;
  type: "text" | "file" | "link" | "group" | string;
  x: number;
  y: number;
  width: number;
  height: number;
  text?: string;
  file?: string;
  url?: string;
  label?: string;
  color?: string;
  /** Marrow: draw this text card as a shape (a kind from shapes.ts). */
  shape?: string;
  /** Marrow: how a shape is filled: none (see-through), solid, or light (unset). */
  fill?: "none" | "solid";
  /** Marrow: a pen stroke or a free arrow. */
  ink?: Ink;
  /** Marrow Home: a live card (tasks, sticky note…) drawn by the Home board. */
  widget?: string;
  /** Marrow Home: the card's settings. */
  cfg?: Record<string, unknown>;
  /** Marrow Home: how the card looks (see home.ts). */
  style?: Record<string, unknown>;
  [k: string]: unknown;
}

/** Strokes: the pen as drawn, straight lines, arrows and the highlighter. */
type StrokeTool = "pen" | "line" | "arrow" | "double" | "highlighter";
type Tool = "select" | "text" | "shape" | StrokeTool;

interface Ink {
  /** The box the points were drawn in (the card scales it). */
  w: number;
  h: number;
  /** Line width. */
  size: number;
  /** "x,y x,y …" inside that box. */
  points: string;
  /** An arrowhead at the end; `arrowStart` one at the start too. */
  arrow?: boolean;
  arrowStart?: boolean;
  /** A highlighter stroke: wide and see-through. */
  highlight?: boolean;
}

interface CEdge {
  id: string;
  fromNode: string;
  toNode: string;
  fromSide?: Side;
  toSide?: Side;
  fromEnd?: "none" | "arrow";
  toEnd?: "none" | "arrow";
  label?: string;
  color?: string;
  [k: string]: unknown;
}

export interface CData {
  nodes: CNode[];
  edges: CEdge[];
  [k: string]: unknown;
}

export interface CanvasApp {
  vaultRoot(): string;
  openPath(path: string, inSplit: boolean): void;
  toast(text: string, kind?: "info" | "error"): void;
}

export interface CanvasOptions {
  /**
   * Drawn inside a note: no title, the page scrolls with the wheel (zoom is
   * Ctrl+wheel), and the tools show while it's being used.
   */
  inline?: boolean;
  /** The Home board: it draws its own live cards and looks. */
  home?: HomeHooks;
}

/** What the Home board adds to a canvas. */
export interface HomeHooks {
  /** Fills a live card (`n.widget`). `changed` saves after the card changed `n`. */
  render(n: CNode, el: HTMLElement, changed: () => void): void;
  /** Gives any card its look (`n.style`). */
  style(n: CNode, el: HTMLElement): void;
  /** The cards picked changed. */
  onSelect?(ids: string[]): void;
  /** Anything on the board changed (moved, resized, added, undone…). */
  onChange?(): void;
}

const IMAGE = /\.(png|jpe?g|gif|webp|svg|bmp|avif)$/i;
const SVGNS = "http://www.w3.org/2000/svg";
/** JSON Canvas colours "1"–"6" (red, orange, yellow, green, cyan, purple). */
const PALETTE: [string, string, string][] = [
  ["1", "Red", "#e03e3e"],
  ["2", "Orange", "#e8780c"],
  ["3", "Yellow", "#d4a106"],
  ["4", "Green", "#2f9e44"],
  ["5", "Cyan", "#0b8cb5"],
  ["6", "Purple", "#8e44ad"],
];
const colorOf = (c?: string) => (c ? (PALETTE.find((p) => p[0] === c)?.[2] ?? (/^#[0-9a-f]{3,8}$/i.test(c) ? c : null)) : null);
const INK_TEXT = "✏️ Drawing — open this canvas in Marrow to see it.";
const isInk = (n: CNode) => n.type === "text" && !!n.ink && typeof n.ink === "object" && typeof n.ink.points === "string";
const isShape = (n: CNode) => n.type === "text" && !!shapeDef(n.shape);
const svgEl = (tag: string, attrs: Record<string, string | number>) => {
  const el = document.createElementNS(SVGNS, tag);
  for (const [k, v] of Object.entries(attrs)) el.setAttribute(k, String(v));
  return el;
};
const TOOLS: [Tool, string, string, string][] = [
  ["select", "cursor", "Select and move", "V"],
  ["text", "text-t", "Text card", "T"],
  ["shape", "shapes", "Shapes", "S"],
  ["line", "line-segment", "Line", "L"],
  ["arrow", "arrow-up-right", "Arrow", "A"],
  ["double", "arrows-horizontal", "Double arrow", "W"],
  ["pen", "pencil-simple", "Pen", "P"],
  ["highlighter", "highlighter", "Highlighter", "H"],
];
/** Keys that pick a shape straight away. */
const SHAPE_KEYS: Record<string, string> = { r: "rect", o: "ellipse", d: "diamond" };
const PEN_SIZES = [2, 4, 8];
const newId = () => Array.from(crypto.getRandomValues(new Uint8Array(8)), (b) => b.toString(16).padStart(2, "0")).join("");

export class CanvasView {
  path: string | null = null;
  private data: CData = { nodes: [], edges: [] };
  private lastSaved = "";
  private saveTimer: number | undefined;
  private saving: Promise<void> | null = null;
  private k = 1;
  private tx = 0;
  private ty = 0;
  private views = new Map<string, { k: number; tx: number; ty: number }>();
  private selected = new Set<string>();
  private selectedEdge: string | null = null;
  private tool: Tool = "select";
  /** The shape the Shapes tool draws. */
  private shapeKind = "rect";
  /** How new shapes are filled. */
  private fill: CNode["fill"] | undefined;
  private shapePop: HTMLElement | null = null;
  private color: string | undefined;
  private penSize = 4;
  /** Undo: the canvas as JSON before each change, and after undoing. */
  private past: string[] = [];
  private future: string[] = [];
  private base = "";
  private lastRecord = 0;
  private edgeClick = 0;
  private toolbar: HTMLElement;
  private draftG: SVGGElement;

  private world: HTMLElement;
  private edgesSvg: SVGSVGElement;
  private cards = new Map<string, HTMLElement>();
  private nameEl: HTMLElement;
  private zoomLabel: HTMLElement;
  private drag:
    | { kind: "pan"; x: number; y: number; moved: boolean }
    | { kind: "move"; x: number; y: number; moved: boolean; start: Map<string, { x: number; y: number }> }
    | { kind: "resize"; id: string; x: number; y: number; w: number; h: number }
    | { kind: "connect"; from: string; x: number; y: number; target: string | null }
    | { kind: "draw"; tool: "shape" | StrokeTool; pts: { x: number; y: number }[]; x: number; y: number }
    | null = null;

  constructor(
    private host: HTMLElement,
    private app: CanvasApp,
    private opts: CanvasOptions = {},
  ) {
    this.world = h("div.canvas-world");
    this.edgesSvg = document.createElementNS(SVGNS, "svg") as SVGSVGElement;
    this.edgesSvg.classList.add("canvas-edges");
    this.edgesSvg.setAttribute("width", "1");
    this.edgesSvg.setAttribute("height", "1");
    this.edgesSvg.innerHTML =
      '<defs><marker id="cv-arrow" viewBox="0 0 10 10" refX="9" refY="5" markerWidth="7" markerHeight="7" orient="auto-start-reverse"><path d="M0,0 L10,5 L0,10 z" style="fill:color-mix(in srgb,var(--color-text) 55%,transparent)"/></marker></defs>';
    this.draftG = svgEl("g", { class: "draw-draft" }) as SVGGElement;
    this.edgesSvg.append(this.draftG);
    this.world.append(this.edgesSvg);
    this.toolbar = this.makeToolbar();
    this.nameEl = h("div.name");
    this.zoomLabel = h("span");
    host.classList.toggle("is-inline", !!opts.inline);
    host.classList.toggle("is-home", !!opts.home);
    host.append(
      this.world,
      opts.inline || opts.home
        ? ""
        : h(
            "div.canvas-over",
            null,
            h("div.smallcaps", null, "Canvas"),
            this.nameEl,
            h("div.canvas-hint", null, "Drag cards to arrange · double-click a note to open it · drop notes here from the sidebar · draw with the tools below"),
          ),
      this.toolbar,
      h(
        "div.canvas-zoom",
        null,
        h("button", { title: "Zoom out", onclick: () => this.zoomBy(1 / 1.25) }, icon("minus")),
        this.zoomLabel,
        h("button", { title: "Zoom in", onclick: () => this.zoomBy(1.25) }, icon("plus")),
        h("button", { title: "Fit to the cards", onclick: () => this.fit() }, icon("corners-out")),
      ),
    );
    this.bind();
  }

  // --------------------------------------------------------------- loading

  async load(path: string) {
    if (this.path) {
      // Save what's here first, even when reopening the same canvas, so
      // recent changes are never replaced by the older file. If they could
      // be neither saved nor kept, stay on this canvas rather than lose them.
      if (!(await this.flush()) && this.unsafe)
        throw new Error(`your changes to “${displayName(this.path)}” couldn't be saved or kept, so it stays open. Copy them somewhere safe, then try again`);
      if (this.path !== path) this.views.set(this.path, { k: this.k, tx: this.tx, ty: this.ty });
    }
    const disk = await api.readNote(path);
    let shown = disk;
    // Changes kept from a save that failed (or a crash) are offered back.
    const kept = await api.backupFor(path).catch(() => null);
    if (kept && kept.text !== disk) {
      const when = new Date(kept.savedAt).toLocaleString(undefined, { dateStyle: "medium", timeStyle: "short" });
      const restore = await ask(`Changes you made to “${displayName(path)}” on ${when} weren't saved, so they were kept. Restore them?`, {
        title: "Unsaved canvas changes",
        kind: "warning",
        okLabel: "Restore",
        cancelLabel: "Discard",
      });
      if (restore) shown = kept.text;
      else void api.discardBackup(path);
    } else if (kept) void api.discardBackup(path);
    this.path = path;
    this.lastSaved = disk;
    this.data = parse(shown);
    if (shown !== disk) this.scheduleSave();
    this.resetUndo();
    this.selected.clear();
    this.selectedEdge = null;
    this.nameEl.textContent = displayName(path);
    this.renderAll();
    const v = this.views.get(path);
    if (v) {
      ({ k: this.k, tx: this.tx, ty: this.ty } = v);
      this.applyView();
    } else requestAnimationFrame(() => this.fit());
  }

  /** The file changed outside (or was renamed). */
  async onDiskChanged() {
    if (!this.path || this.saveTimer !== undefined || this.saving) return;
    const text = await api.readNote(this.path).catch(() => null);
    if (text === null || text === this.lastSaved) return;
    this.lastSaved = text;
    this.data = parse(text);
    this.resetUndo();
    this.renderAll();
  }

  renamed(path: string) {
    this.path = path;
    this.nameEl.textContent = displayName(path);
  }

  private changed() {
    this.record();
    this.scheduleSave();
    this.opts.home?.onChange?.();
  }

  /** Counts edits, so a conflict can tell whether more were made meanwhile. */
  private rev = 0;
  /** The latest changes are neither saved nor kept as a draft. */
  private unsafe = false;

  private serialize(): string {
    return JSON.stringify(this.data, null, "\t");
  }

  private scheduleSave() {
    this.rev++;
    window.clearTimeout(this.saveTimer);
    this.saveTimer = window.setTimeout(() => void this.flush(), 600);
  }

  // ------------------------------------------------------------------ undo

  private resetUndo() {
    this.past = [];
    this.future = [];
    this.base = JSON.stringify(this.data);
  }

  /** Remembers the state before this change (quick runs of changes count as one). */
  private record() {
    const now = JSON.stringify(this.data);
    if (now === this.base) return;
    if (Date.now() - this.lastRecord > 800 || this.past.length === 0) {
      this.past.push(this.base);
      if (this.past.length > 100) this.past.shift();
    }
    this.lastRecord = Date.now();
    this.base = now;
    this.future = [];
  }

  undo() {
    const prev = this.past.pop();
    if (prev === undefined) return;
    this.future.push(this.base);
    this.restore(prev);
  }

  redo() {
    const next = this.future.pop();
    if (next === undefined) return;
    this.past.push(this.base);
    this.restore(next);
  }

  private restore(json: string) {
    this.base = json;
    this.lastRecord = 0;
    this.data = parse(json);
    this.selected.clear();
    this.selectedEdge = null;
    this.renderAll();
    this.scheduleSave();
    this.opts.home?.onSelect?.([]);
    this.opts.home?.onChange?.();
  }

  /**
   * Saves now. True once the canvas on screen is on disk (or kept safe),
   * including edits made while an earlier save was still writing.
   */
  async flush(): Promise<boolean> {
    for (let round = 0; round < 20; round++) {
      if (!(await this.flushOnce())) return false;
      if (!this.path || this.serialize() === this.lastSaved) return true;
    }
    return false;
  }

  /** True when the canvas has changes that are neither saved nor kept. */
  get changesAtRisk(): boolean {
    return this.unsafe;
  }

  private async flushOnce(): Promise<boolean> {
    window.clearTimeout(this.saveTimer);
    this.saveTimer = undefined;
    if (this.saving) await this.saving;
    if (!this.path) return true;
    const text = this.serialize();
    if (text === this.lastSaved) return true;
    const path = this.path;
    const rev = this.rev;
    let ok = false;
    this.saving = api
      // Only replace the version we loaded; another app may have changed it.
      .writeNote(path, text, this.lastSaved)
      .then(() => {
        this.lastSaved = text;
        this.unsafe = false;
        ok = true;
      })
      .catch(async (e) => {
        if (String(e) === "CHANGED_ON_DISK") {
          // Another view of the same drawing may have saved exactly this.
          const disk = await api.readNote(path).catch(() => null);
          if (disk !== null && sameBoard(disk, text)) {
            this.lastSaved = disk;
            this.unsafe = false;
            ok = true;
            return;
          }
          ok = await this.keepBoth(path, text, rev);
          return;
        }
        // Kept outside the vault and offered back when the canvas opens.
        this.unsafe = !(await api.keepDraft(path, text).then(
          () => true,
          () => false,
        ));
        this.app.toast(`Couldn't save the canvas: ${e}`, "error");
      })
      .finally(() => {
        this.saving = null;
      });
    await this.saving;
    return ok;
  }

  /**
   * Another app changed the canvas while it had changes here. Neither side
   * is thrown away: these changes go to a copy beside it, and the canvas
   * shows the other app's version. Edits made while that happens go into
   * the copy too, before the canvas switches over.
   */
  private async keepBoth(path: string, mine: string, rev: number): Promise<boolean> {
    const folder = parentOf(path);
    let copy: string;
    try {
      copy = await api.createFile(folder, `${displayName(path)} (my changes)`, "canvas", mine);
    } catch (e) {
      this.unsafe = !(await api.keepDraft(path, mine).then(
        () => true,
        () => false,
      ));
      this.app.toast(`“${displayName(path)}” was changed in another app, and your changes couldn't be saved beside it: ${e}`, "error");
      return false;
    }
    for (let round = 0; round < 20 && this.path === path; round++) {
      if (this.rev !== rev) {
        // Edited while the copy was being made: bring the copy up to date.
        rev = this.rev;
        try {
          await api.writeNote(copy, this.serialize());
        } catch (e) {
          this.unsafe = true;
          this.app.toast(`Couldn't update “${displayName(copy)}” with your latest changes: ${e}`, "error");
          return false;
        }
        continue;
      }
      const disk = await api.readNote(path).catch(() => null);
      if (this.rev !== rev) continue; // edited while reading: copy first
      if (disk !== null) {
        window.clearTimeout(this.saveTimer);
        this.saveTimer = undefined;
        this.lastSaved = disk;
        this.data = parse(disk);
        this.resetUndo();
        this.renderAll();
      }
      break;
    }
    this.unsafe = false;
    this.app.toast(`“${displayName(path)}” was changed in another app. Your changes were saved as “${displayName(copy)}”.`);
    return true;
  }

  // ---------------------------------------------------------------- render

  private renderAll() {
    for (const el of this.cards.values()) el.remove();
    this.cards.clear();
    // Groups underneath everything else.
    const order = [...this.data.nodes].sort((a, b) => Number(b.type === "group") - Number(a.type === "group"));
    for (const n of order) this.world.append(this.card(n));
    this.renderEdges();
    this.applyView();
  }

  private card(n: CNode): HTMLElement {
    let el: HTMLElement;
    if (n.widget && this.opts.home) {
      el = h("div.card.card-widget");
      el.dataset.widget = n.widget;
      this.opts.home.render(n, el, () => this.changed());
    } else if (isInk(n)) {
      el = this.inkCard(n);
    } else if (isShape(n)) {
      el = this.shapeCard(n);
    } else if (n.type === "text") {
      const ta = h("textarea", { spellcheck: false, placeholder: "Write something…" }) as HTMLTextAreaElement;
      ta.value = n.text ?? "";
      ta.addEventListener("input", () => {
        n.text = ta.value;
        this.changed();
      });
      ta.addEventListener("pointerdown", (e) => {
        // Typing, not dragging; but the card is still the one selected.
        e.stopPropagation();
        if (!this.selected.has(n.id)) this.select([n.id]);
      });
      el = h("div.card.card-text", null, h("div.k", null, icon("dots-six"), "Text card"), ta);
    } else if (n.type === "file" && n.file) {
      const file = n.file;
      if (IMAGE.test(file)) {
        el = h("div.card.card-note", { style: "padding:8px" }, h("img", { src: this.assetUrl(file), alt: displayName(file), style: "width:100%;height:100%;object-fit:contain;pointer-events:none" }));
      } else {
        const ex = h("div.x");
        el = h(
          "div.card.card-note",
          { title: file },
          h("div.k", null, icon(file.endsWith(".canvas") ? "squares-four" : "file-text"), parentOf(file) || "Vault"),
          h("div.t", null, displayName(file)),
          ex,
        );
        void api
          .readNote(file)
          .then((t) => (ex.textContent = trunc(plainText(t), 220)))
          .catch((e) => {
            if (String(e) === "LOCKED") {
              ex.textContent = "🔒 Locked note";
              return;
            }
            el.classList.add("missing");
            ex.textContent = "This note was moved or deleted.";
          });
      }
    } else if (n.type === "link") {
      el = h(
        "div.card.card-link",
        null,
        h("div.k", null, icon("link-simple"), "Link"),
        h("a", { href: "#", onclick: (e: Event) => (e.preventDefault(), void openUrl(n.url ?? "").catch(() => {})) }, n.url ?? ""),
      );
    } else if (n.type === "group") {
      el = h("div.card.card-group", null, h("div.k", null, n.label ?? "Group"));
    } else {
      el = h("div.card.card-link", null, h("div.k", null, icon("question"), n.type));
    }
    el.dataset.id = n.id;
    this.place(el, n);
    const c = colorOf(n.color);
    if (c) el.style.setProperty("--c", c);
    el.classList.toggle("has-color", !!c);
    this.opts.home?.style(n, el);
    if (n.type !== "group" && !isInk(n) && !this.opts.home) el.append(h("div.card-handle", { title: "Drag to another card to connect" }));
    el.append(h("div.card-resize"));
    el.classList.toggle("is-selected", this.selected.has(n.id));
    this.cards.set(n.id, el);
    return el;
  }

  private place(el: HTMLElement, n: CNode) {
    el.style.left = `${n.x}px`;
    el.style.top = `${n.y}px`;
    el.style.width = `${n.width}px`;
    el.style.height = `${n.height}px`;
    // Whole lines of excerpt only.
    const ex = el.querySelector<HTMLElement>(".card-note .x");
    if (ex) ex.style.setProperty("-webkit-line-clamp", String(Math.max(1, Math.floor((n.height - 84) / 21))));
  }

  /** A shape with a label (double-click to edit it). */
  private shapeCard(n: CNode): HTMLElement {
    const def = shapeDef(n.shape)!;
    const svg = svgEl("svg", { class: "shape-svg", viewBox: "0 0 100 100", preserveAspectRatio: "none" });
    const attrs = { "vector-effect": "non-scaling-stroke" };
    svg.append(svgEl("path", { ...attrs, class: "shape-fill", d: def.d }));
    if (def.extra) svg.append(svgEl("path", { ...attrs, class: "shape-line", d: def.extra }));
    // See-through shapes are picked by their outline, so what's under them can be picked too.
    svg.append(svgEl("path", { ...attrs, class: "shape-hit", d: def.d }));
    const label = h("div.shape-label", { spellcheck: false });
    label.textContent = n.text ?? "";
    const [x0, y0, x1, y1] = def.label;
    label.style.cssText = `left:${x0}%;top:${y0}%;width:${x1 - x0}%;height:${y1 - y0}%`;
    label.addEventListener("input", () => {
      n.text = label.innerText.replace(/\n$/, "");
      this.changed();
    });
    label.addEventListener("pointerdown", (e) => {
      if (label.isContentEditable) e.stopPropagation();
    });
    label.addEventListener("keydown", (e) => {
      e.stopPropagation(); // typing, not tool shortcuts
      if (e.key === "Escape" || (e.key === "Enter" && (e.ctrlKey || e.metaKey))) {
        e.preventDefault();
        label.blur();
        this.host.focus({ preventScroll: true });
      }
    });
    label.addEventListener("blur", () => {
      label.contentEditable = "false";
      window.getSelection()?.removeAllRanges();
    });
    return h("div.card.card-shape", { dataset: { shape: n.shape ?? "rect", fill: n.fill ?? "light" } }, svg, label);
  }

  private editLabel(id: string) {
    const label = this.cards.get(id)?.querySelector<HTMLElement>(".shape-label");
    if (!label) return;
    try {
      label.contentEditable = "plaintext-only";
    } catch {
      label.contentEditable = "true";
    }
    label.focus();
    const r = document.createRange();
    r.selectNodeContents(label);
    const sel = window.getSelection();
    sel?.removeAllRanges();
    sel?.addRange(r);
  }

  /** A pen stroke or an arrow, drawn in its own box. */
  private inkCard(n: CNode): HTMLElement {
    const ink = n.ink!;
    const svg = svgEl("svg", { class: "ink-svg", viewBox: `0 0 ${ink.w} ${ink.h}`, preserveAspectRatio: "none" });
    const d = inkPath(ink);
    svg.append(svgEl("path", { class: ink.highlight ? "ink-line ink-hl" : "ink-line", d, "stroke-width": ink.size }), svgEl("path", { class: "ink-hit", d }));
    return h("div.card.card-ink", null, svg);
  }

  private assetUrl(path: string) {
    const root = this.app.vaultRoot();
    const sep = root.includes("\\") ? "\\" : "/";
    return convertFileSrc(root.replace(/[\\/]+$/, "") + sep + path.split("/").join(sep));
  }

  private node(id: string) {
    return this.data.nodes.find((n) => n.id === id);
  }

  private renderEdges(draft?: { from: CNode; x: number; y: number }) {
    const svg = this.edgesSvg;
    for (const el of [...svg.querySelectorAll(":scope > path, :scope > text")]) el.remove();
    const mk = (d: string, cls: string, edge?: CEdge) => {
      const p = document.createElementNS(SVGNS, "path");
      p.setAttribute("d", d);
      if (cls) p.setAttribute("class", cls);
      if (edge) {
        const from = this.node(edge.fromNode);
        const to = this.node(edge.toNode);
        const plainText = (n?: CNode) => n?.type === "text" && !isShape(n);
        if (plainText(from) || plainText(to)) p.setAttribute("stroke-dasharray", "5 5");
        if ((edge.toEnd ?? "arrow") === "arrow") p.setAttribute("marker-end", "url(#cv-arrow)");
        if (edge.fromEnd === "arrow") p.setAttribute("marker-start", "url(#cv-arrow)");
        const c = colorOf(edge.color);
        if (c && cls !== "sel") p.style.stroke = c;
      }
      svg.append(p);
      return p;
    };
    for (const e of this.data.edges) {
      const a = this.node(e.fromNode);
      const b = this.node(e.toNode);
      if (!a || !b) continue;
      const fs = e.fromSide ?? bestSide(a, b);
      const ts = e.toSide ?? bestSide(b, a);
      const d = curve(anchor(a, fs), fs, anchor(b, ts), ts);
      mk(d, e.id === this.selectedEdge ? "sel" : "", e);
      const hit = mk(d, "hit");
      hit.dataset.edge = e.id;
      hit.addEventListener("pointerdown", (ev) => {
        ev.stopPropagation();
        // A second click on the same arrow edits its label (the arrow is
        // redrawn on the first click, so dblclick wouldn't reach it).
        const again = this.selectedEdge === e.id && Date.now() - this.edgeClick < 450;
        this.edgeClick = Date.now();
        if (again) {
          ev.preventDefault();
          this.editEdgeLabel(e.id);
        } else if (this.selectedEdge !== e.id) this.selectEdge(e.id);
      });
      if (e.label) {
        const m = mid(anchor(a, fs), anchor(b, ts));
        const t = document.createElementNS(SVGNS, "text");
        t.setAttribute("x", String(m.x));
        t.setAttribute("y", String(m.y - 6));
        t.setAttribute("text-anchor", "middle");
        t.setAttribute("style", "font-size:13px;fill:var(--color-text);paint-order:stroke;stroke:var(--color-bg);stroke-width:4px");
        t.textContent = e.label;
        svg.insertBefore(t, this.draftG);
      }
    }
    if (draft) {
      const fs: Side = "right";
      mk(curve(anchor(draft.from, fs), fs, { x: draft.x, y: draft.y }, "left"), "draft");
    }
  }

  private applyView() {
    this.world.style.transform = `translate(${this.tx}px, ${this.ty}px) scale(${this.k})`;
    this.host.style.backgroundPosition = `${this.tx}px ${this.ty}px`;
    this.host.style.backgroundSize = `${22 * this.k}px ${22 * this.k}px`;
    this.zoomLabel.textContent = `${Math.round(this.k * 100)}%`;
  }

  fit() {
    const r = this.host.getBoundingClientRect();
    if (this.data.nodes.length === 0) {
      this.k = 1;
      this.tx = r.width / 2 - 140;
      this.ty = r.height / 2 - 80;
      this.applyView();
      return;
    }
    let x0 = Infinity, y0 = Infinity, x1 = -Infinity, y1 = -Infinity;
    for (const n of this.data.nodes) {
      x0 = Math.min(x0, n.x);
      y0 = Math.min(y0, n.y);
      x1 = Math.max(x1, n.x + n.width);
      y1 = Math.max(y1, n.y + n.height);
    }
    const [padX, padTop, padBottom] = this.opts.inline ? [24, 56, 24] : [60, 130, 80];
    const k = Math.min((r.width - padX * 2) / (x1 - x0), (r.height - padTop - padBottom) / (y1 - y0), 1);
    this.k = Math.max(0.1, k);
    this.tx = (r.width - (x1 - x0) * this.k) / 2 - x0 * this.k;
    this.ty = padTop + (r.height - padTop - padBottom - (y1 - y0) * this.k) / 2 - y0 * this.k;
    this.applyView();
  }

  private zoomBy(f: number, cx?: number, cy?: number) {
    const r = this.host.getBoundingClientRect();
    const mx = cx ?? r.width / 2;
    const my = cy ?? r.height / 2;
    const k = Math.min(3, Math.max(0.1, this.k * f));
    this.tx = mx - ((mx - this.tx) * k) / this.k;
    this.ty = my - ((my - this.ty) * k) / this.k;
    this.k = k;
    this.applyView();
  }

  private toWorld(cx: number, cy: number) {
    const r = this.host.getBoundingClientRect();
    return { x: (cx - r.left - this.tx) / this.k, y: (cy - r.top - this.ty) / this.k };
  }

  // ---------------------------------------------------------------- toolbar

  private makeToolbar(): HTMLElement {
    const tools = TOOLS.map(([t, ic, label, key]) =>
      t === "shape"
        ? h(
            "button.cv-tool.cv-shape-btn",
            { type: "button", title: `${label} (${key}): pick one`, dataset: { tool: t }, onclick: (e: Event) => this.toggleShapes(e.currentTarget as HTMLElement) },
            shapeIcon(this.shapeKind),
            h("span.cv-caret", null, icon("caret-down")),
          )
        : h("button.cv-tool", { type: "button", title: `${label} (${key})`, dataset: { tool: t }, onclick: () => this.setTool(t) }, icon(ic)),
    );
    const fills = FILLS.map(([f, label, ic]) =>
      h("button.cv-tool.cv-fill", { type: "button", title: label, dataset: { fill: f }, onclick: () => this.setFill((f || undefined) as CNode["fill"]) }, icon(ic)),
    );
    const swatches = [
      h("button.cv-swatch.is-none", { type: "button", title: "No colour", dataset: { color: "" }, onclick: () => this.setColor(undefined) }),
      ...PALETTE.map(([id, name, hex]) =>
        h("button.cv-swatch", { type: "button", title: name, dataset: { color: id }, style: `--sw:${hex}`, onclick: () => this.setColor(id) }),
      ),
    ];
    const sizes = PEN_SIZES.map((sz) =>
      h(
        "button.cv-size",
        { type: "button", title: `Line ${sz === 2 ? "thin" : sz === 4 ? "medium" : "thick"}`, dataset: { size: String(sz) }, onclick: () => this.setPenSize(sz) },
        h("span", { style: `width:${sz + 3}px;height:${sz + 3}px` }),
      ),
    );
    const bar = h(
      "div.canvas-tools",
      null,
      h("div.cv-bar", null, ...tools),
      h("div.cv-bar", null, ...swatches, h("span.cv-fills", null, h("span.cv-sep"), ...fills), h("span.cv-pen-sizes", null, h("span.cv-sep"), ...sizes)),
      h(
        "div.cv-bar",
        null,
        h("button.cv-tool", { type: "button", title: "Undo (Ctrl+Z)", onclick: () => this.undo() }, icon("arrow-counter-clockwise")),
        h("button.cv-tool", { type: "button", title: "Redo (Ctrl+Shift+Z)", onclick: () => this.redo() }, icon("arrow-clockwise")),
      ),
    );
    queueMicrotask(() => this.syncToolbar());
    return bar;
  }

  private syncToolbar() {
    for (const b of this.toolbar.querySelectorAll<HTMLElement>(".cv-tool[data-tool]")) b.classList.toggle("is-on", b.dataset.tool === this.tool);
    // The swatch shows the selection's colour, or the colour for new drawings.
    const first = [...this.selected].map((id) => this.node(id)).find(Boolean);
    const edge = this.selectedEdge ? this.data.edges.find((e) => e.id === this.selectedEdge) : undefined;
    const shown = first ? (first.color ?? "") : edge ? (edge.color ?? "") : (this.color ?? "");
    for (const b of this.toolbar.querySelectorAll<HTMLElement>(".cv-swatch")) b.classList.toggle("is-on", b.dataset.color === shown);
    for (const b of this.toolbar.querySelectorAll<HTMLElement>(".cv-size")) b.classList.toggle("is-on", Number(b.dataset.size) === this.penSize);
    // The fill shows the selected shape's, or the one for new shapes.
    const firstShape = [...this.selected].map((id) => this.node(id)).find((n) => n && isShape(n));
    const fillShown = firstShape ? (firstShape.fill ?? "") : (this.fill ?? "");
    for (const b of this.toolbar.querySelectorAll<HTMLElement>(".cv-fill")) b.classList.toggle("is-on", b.dataset.fill === fillShown);
    this.toolbar.classList.toggle("is-pen", this.tool === "pen" || this.tool === "highlighter");
    this.toolbar.classList.toggle("is-shape", this.tool === "shape" || !!firstShape);
    const sb = this.toolbar.querySelector<HTMLElement>(".cv-shape-btn");
    if (sb && sb.dataset.kind !== this.shapeKind) {
      sb.dataset.kind = this.shapeKind;
      sb.firstElementChild?.replaceWith(shapeIcon(this.shapeKind));
      sb.title = `${shapeDef(this.shapeKind)?.name ?? "Shapes"} (S): pick another`;
    }
    for (const t of TOOLS) this.host.classList.toggle(`tool-${t[0]}`, t[0] === this.tool);
  }

  setTool(t: Tool) {
    this.tool = t;
    this.shapePop?.remove();
    this.shapePop = null;
    this.syncToolbar();
  }

  /** Draw this shape next. */
  setShape(kind: string) {
    if (shapeDef(kind)) this.shapeKind = kind;
    this.setTool("shape");
  }

  /** The shape picker: every shape, drawn as itself. */
  private toggleShapes(anchor: HTMLElement) {
    if (this.shapePop) {
      this.shapePop.remove();
      this.shapePop = null;
      return;
    }
    const pop = h(
      "div.cv-shapes",
      { role: "menu", "aria-label": "Shapes" },
      ...SHAPES.map((sd) =>
        h(
          "button.cv-shape",
          { type: "button", role: "menuitem", title: sd.name, "aria-label": sd.name, onclick: () => this.setShape(sd.kind) },
          shapeIcon(sd.kind),
        ),
      ),
    );
    pop.querySelector<HTMLElement>(`.cv-shape:nth-child(${SHAPES.findIndex((x) => x.kind === this.shapeKind) + 1})`)?.classList.add("is-on");
    this.toolbar.append(pop);
    const bar = this.toolbar.getBoundingClientRect();
    const a = anchor.getBoundingClientRect();
    pop.style.left = `${a.left - bar.left}px`;
    // Below the toolbar when it sits at the top (in a note), above it otherwise.
    if (this.opts.inline) pop.style.top = `${a.bottom - bar.top + 6}px`;
    else pop.style.bottom = `${bar.bottom - a.top + 6}px`;
    this.shapePop = pop;
  }

  /** How shapes are filled: the selected ones, and the next ones drawn. */
  private setFill(f: CNode["fill"] | undefined) {
    this.fill = f;
    let touched = false;
    for (const id of this.selected) {
      const n = this.node(id);
      if (!n || !isShape(n)) continue;
      if (f) n.fill = f;
      else delete n.fill;
      touched = true;
    }
    if (touched) {
      const keep = [...this.selected];
      this.renderAll();
      this.select(keep);
      this.changed();
    }
    this.syncToolbar();
  }

  /** Ready to draw: the pen, with the keyboard here. */
  startDrawing() {
    this.setTool("pen");
    this.host.focus({ preventScroll: true });
  }

  get isEmpty(): boolean {
    return this.data.nodes.length === 0;
  }

  private setPenSize(sz: number) {
    this.penSize = sz;
    if (this.tool !== "highlighter") this.setTool("pen");
  }

  /** Colours what's selected, and what's drawn next. */
  private setColor(c: string | undefined) {
    this.color = c;
    let touched = false;
    for (const id of this.selected) {
      const n = this.node(id);
      if (!n) continue;
      if (c) n.color = c;
      else delete n.color;
      touched = true;
    }
    const edge = this.selectedEdge ? this.data.edges.find((e) => e.id === this.selectedEdge) : undefined;
    if (edge) {
      if (c) edge.color = c;
      else delete edge.color;
      touched = true;
    }
    if (touched) {
      const keep = [...this.selected];
      const keepEdge = this.selectedEdge;
      this.renderAll();
      if (keepEdge) this.selectEdge(keepEdge);
      else this.select(keep);
      this.changed();
    }
    this.syncToolbar();
  }

  // ---------------------------------------------------------------- editing

  addText(at?: { x: number; y: number }) {
    const r = this.host.getBoundingClientRect();
    const c = at ?? this.toWorld(r.left + r.width / 2, r.top + r.height / 2);
    const n: CNode = { id: newId(), type: "text", text: "", x: Math.round(c.x - 140), y: Math.round(c.y - 60), width: 280, height: 120 };
    this.data.nodes.push(n);
    this.world.append(this.card(n));
    this.select([n.id]);
    this.changed();
    requestAnimationFrame(() => this.cards.get(n.id)?.querySelector("textarea")?.focus());
  }

  addFile(path: string, at: { x: number; y: number }) {
    const img = IMAGE.test(path);
    const n: CNode = {
      id: newId(),
      type: "file",
      file: path,
      x: Math.round(at.x - 140),
      y: Math.round(at.y - 40),
      width: img ? 320 : 280,
      height: img ? 240 : 160,
    };
    this.data.nodes.push(n);
    this.world.append(this.card(n));
    this.select([n.id]);
    this.changed();
  }

  private select(ids: string[]) {
    this.selected = new Set(ids);
    this.selectedEdge = null;
    for (const [id, el] of this.cards) el.classList.toggle("is-selected", this.selected.has(id));
    this.renderEdges();
    this.syncToolbar();
    this.opts.home?.onSelect?.([...this.selected]);
  }

  // ------------------------------------------------- for the Home board

  /** Every card (live objects: change them, then call `patched`). */
  get nodes(): CNode[] {
    return this.data.nodes;
  }

  /** The board's own settings (`home` in the file). */
  get board(): Record<string, unknown> {
    return (this.data.home ??= {}) as Record<string, unknown>;
  }

  get selection(): string[] {
    return [...this.selected];
  }

  /** Puts a card on the board, centred where the view is unless it has a place. */
  addCard(n: CNode, pick = true) {
    if (!Number.isFinite(n.x) || !Number.isFinite(n.y)) {
      const r = this.host.getBoundingClientRect();
      const c = this.toWorld(r.left + r.width / 2, r.top + r.height / 2);
      n.x = Math.round(c.x - n.width / 2);
      n.y = Math.round(c.y - n.height / 2);
    }
    this.data.nodes.push(n);
    this.world.append(this.card(n));
    if (pick) this.select([n.id]);
    this.changed();
  }

  /** A card's settings changed: draw it again and save. */
  patched(id: string, save = true) {
    const n = this.node(id);
    const old = this.cards.get(id);
    if (!n || !old) return;
    const el = this.card(n);
    old.replaceWith(el);
    if (save) this.changed();
  }

  /** Draws every live card again (tasks or notes changed). */
  refreshLive() {
    for (const n of this.data.nodes) if (n.widget) this.patched(n.id, false);
  }

  /** Brings a card to the front (or sends it to the back). */
  reorder(id: string, front: boolean) {
    const i = this.data.nodes.findIndex((n) => n.id === id);
    if (i < 0) return;
    const [n] = this.data.nodes.splice(i, 1);
    if (front) this.data.nodes.push(n);
    else this.data.nodes.unshift(n);
    this.renderAll();
    this.changed();
  }

  removeCards(ids: string[]) {
    this.select(ids);
    this.deleteSelection();
  }

  /** The whole board replaced (a template): one undo step brings it back. */
  replaceAll(data: CData) {
    this.data = data;
    this.selected.clear();
    this.renderAll();
    this.changed();
    requestAnimationFrame(() => this.fit());
  }

  /** The board as it would be saved. */
  snapshot(): CData {
    return JSON.parse(this.serialize()) as CData;
  }

  /** Saves after the board's own settings changed. */
  touch() {
    this.changed();
  }

  /** The board's background was changed: the dots/grid follow the view. */
  redrawView() {
    this.applyView();
  }

  private selectEdge(id: string) {
    this.selected.clear();
    for (const el of this.cards.values()) el.classList.remove("is-selected");
    this.selectedEdge = id;
    this.renderEdges();
    this.syncToolbar();
  }

  private selectAll() {
    this.select(this.data.nodes.map((n) => n.id));
  }

  /** Finishes a shape, a line, an arrow or a pen stroke. */
  private finishDrawing(tool: "shape" | StrokeTool, pts: { x: number; y: number }[]) {
    this.draftG.replaceChildren();
    const a = pts[0];
    const b = pts[pts.length - 1];
    let n: CNode | null = null;
    if (tool !== "shape") {
      const straight = tool === "line" || tool === "arrow" || tool === "double";
      const heads = tool === "arrow" || tool === "double";
      let ps = straight ? [a, b] : simplify(pts, 0.8 / this.k);
      if (straight && Math.hypot(b.x - a.x, b.y - a.y) < 8) return;
      if (ps.length === 1) ps = [ps[0], { x: ps[0].x + 0.5, y: ps[0].y + 0.5 }];
      const size = heads ? 2.5 : tool === "highlighter" ? Math.max(14, this.penSize * 4) : this.penSize;
      const pad = size + (heads ? 12 : 4);
      const x0 = Math.min(...ps.map((p) => p.x)) - pad;
      const y0 = Math.min(...ps.map((p) => p.y)) - pad;
      const w = Math.max(...ps.map((p) => p.x)) + pad - x0;
      const hgt = Math.max(...ps.map((p) => p.y)) + pad - y0;
      const r = (v: number) => Math.round(v * 10) / 10;
      n = {
        id: newId(),
        type: "text",
        text: INK_TEXT,
        x: Math.round(x0),
        y: Math.round(y0),
        width: Math.max(1, Math.round(w)),
        height: Math.max(1, Math.round(hgt)),
        ink: {
          w: Math.max(1, Math.round(w)),
          h: Math.max(1, Math.round(hgt)),
          size,
          points: ps.map((p) => `${r(p.x - Math.round(x0))},${r(p.y - Math.round(y0))}`).join(" "),
          ...(heads ? { arrow: true } : {}),
          ...(tool === "double" ? { arrowStart: true } : {}),
          ...(tool === "highlighter" ? { highlight: true } : {}),
        },
      };
    } else {
      const small = Math.abs(b.x - a.x) < 8 && Math.abs(b.y - a.y) < 8;
      const box = small
        ? { x: a.x - 80, y: a.y - 50, w: 160, h: 100 }
        : { x: Math.min(a.x, b.x), y: Math.min(a.y, b.y), w: Math.abs(b.x - a.x), h: Math.abs(b.y - a.y) };
      n = { id: newId(), type: "text", text: "", shape: this.shapeKind, x: Math.round(box.x), y: Math.round(box.y), width: Math.max(40, Math.round(box.w)), height: Math.max(30, Math.round(box.h)) };
      if (this.fill) n.fill = this.fill;
    }
    if (this.color) n.color = this.color;
    this.data.nodes.push(n);
    this.world.append(this.card(n));
    this.changed();
    if (tool === "pen" || tool === "highlighter") return; // keep drawing
    this.select([n.id]);
    this.setTool("select");
    if (isShape(n)) requestAnimationFrame(() => this.editLabel(n!.id));
  }

  private drawDraft(tool: "shape" | StrokeTool, pts: { x: number; y: number }[]) {
    const a = pts[0];
    const b = pts[pts.length - 1];
    const c = colorOf(this.color) ?? "";
    let el: SVGElement;
    if (tool === "pen" || tool === "highlighter") {
      el = svgEl("path", { d: smooth(pts), class: tool === "highlighter" ? "ink-draft ink-hl" : "ink-draft", "stroke-width": tool === "highlighter" ? Math.max(14, this.penSize * 4) : this.penSize });
    } else if (tool === "line" || tool === "arrow" || tool === "double") {
      const d = tool === "line" ? `M${a.x},${a.y} L${b.x},${b.y}` : arrowPath(a, b, 2.5, tool === "double");
      el = svgEl("path", { d, class: "ink-draft", "stroke-width": 2.5 });
    } else {
      const x = Math.min(a.x, b.x), y = Math.min(a.y, b.y), w = Math.abs(b.x - a.x), hh = Math.abs(b.y - a.y);
      el = svgEl("path", { d: placePath(shapeDef(this.shapeKind)!.d, x, y, w, hh), class: "shape-draft" });
    }
    if (c) el.style.stroke = c;
    this.draftG.replaceChildren(el);
  }

  /** A label on an arrow between cards: a box at its middle. */
  private editEdgeLabel(id: string) {
    const e = this.data.edges.find((x) => x.id === id);
    const a = e && this.node(e.fromNode);
    const b = e && this.node(e.toNode);
    if (!e || !a || !b) return;
    const m = mid(anchor(a, e.fromSide ?? bestSide(a, b)), anchor(b, e.toSide ?? bestSide(b, a)));
    const input = h("input.cv-edge-input", { value: e.label ?? "", placeholder: "Label", spellcheck: false, style: `left:${m.x}px;top:${m.y}px` }) as HTMLInputElement;
    let done = false;
    const finish = (save: boolean) => {
      if (done) return;
      done = true;
      input.remove();
      if (save && input.value.trim() !== (e.label ?? "")) {
        if (input.value.trim()) e.label = input.value.trim();
        else delete e.label;
        this.renderEdges();
        this.changed();
      }
      this.host.focus({ preventScroll: true });
    };
    input.addEventListener("keydown", (ev) => {
      ev.stopPropagation();
      if (ev.key === "Enter") finish(true);
      else if (ev.key === "Escape") finish(false);
    });
    input.addEventListener("pointerdown", (ev) => ev.stopPropagation());
    input.addEventListener("blur", () => finish(true));
    this.world.append(input);
    input.focus();
    input.select();
  }

  private deleteSelection() {
    if (this.selectedEdge) {
      this.data.edges = this.data.edges.filter((e) => e.id !== this.selectedEdge);
      this.selectedEdge = null;
    } else if (this.selected.size) {
      this.data.nodes = this.data.nodes.filter((n) => !this.selected.has(n.id));
      this.data.edges = this.data.edges.filter((e) => !this.selected.has(e.fromNode) && !this.selected.has(e.toNode));
      for (const id of this.selected) {
        this.cards.get(id)?.remove();
        this.cards.delete(id);
      }
      this.selected.clear();
      this.opts.home?.onSelect?.([]);
    } else return;
    this.renderEdges();
    this.changed();
  }

  // ------------------------------------------------------------ interaction

  private bind() {
    const host = this.host;
    host.tabIndex = 0;
    host.addEventListener("pointerdown", (e) => {
      if (e.button !== 0) return;
      const t = e.target as HTMLElement;
      if (this.shapePop && !t.closest(".cv-shapes, .cv-shape-btn")) {
        this.shapePop.remove();
        this.shapePop = null;
      }
      if (t.closest(".canvas-tools, .canvas-zoom, a")) return;
      host.focus({ preventScroll: true });
      if (this.tool !== "select") {
        const w = this.toWorld(e.clientX, e.clientY);
        if (this.tool === "text") {
          this.addText(w);
          this.setTool("select");
        } else {
          this.select([]);
          this.drag = { kind: "draw", tool: this.tool, pts: [w], x: e.clientX, y: e.clientY };
          host.setPointerCapture(e.pointerId);
        }
        e.preventDefault();
        return;
      }
      // Live parts of a Home card (a checkbox, a text box, a link) are used, not dragged.
      const live = t.closest<HTMLElement>("[data-live]") ?? (t.closest(".card-widget") ? t.closest<HTMLElement>("button, input, textarea, select, a, label") : null);
      if (live) {
        const id = live.closest<HTMLElement>(".card")?.dataset.id;
        if (id && !this.selected.has(id)) this.select([id]);
        return;
      }
      const cardEl = t.closest<HTMLElement>(".card");
      if (!cardEl) {
        this.select([]);
        this.drag = { kind: "pan", x: e.clientX, y: e.clientY, moved: false };
        host.classList.add("is-panning");
      } else {
        const id = cardEl.dataset.id!;
        const n = this.node(id);
        if (!n) return;
        if (t.classList.contains("card-handle")) {
          this.drag = { kind: "connect", from: id, x: e.clientX, y: e.clientY, target: null };
        } else if (t.classList.contains("card-resize")) {
          this.drag = { kind: "resize", id, x: e.clientX, y: e.clientY, w: n.width, h: n.height };
        } else {
          if (!this.selected.has(id)) this.select(e.shiftKey ? [...this.selected, id] : [id]);
          const ids = new Set(this.selected);
          // Moving a group carries what's inside it.
          for (const sid of [...ids]) {
            const g = this.node(sid);
            if (g?.type !== "group") continue;
            for (const m of this.data.nodes)
              if (m.id !== g.id && m.x >= g.x && m.y >= g.y && m.x + m.width <= g.x + g.width && m.y + m.height <= g.y + g.height) ids.add(m.id);
          }
          const start = new Map([...ids].map((i) => [i, { x: this.node(i)!.x, y: this.node(i)!.y }]));
          this.drag = { kind: "move", x: e.clientX, y: e.clientY, moved: false, start };
        }
      }
      host.setPointerCapture(e.pointerId);
      e.preventDefault();
    });
    host.addEventListener("pointermove", (e) => {
      const d = this.drag;
      if (!d) return;
      const dx = e.clientX - d.x;
      const dy = e.clientY - d.y;
      if (d.kind === "draw") {
        const w = this.toWorld(e.clientX, e.clientY);
        const last = d.pts[d.pts.length - 1];
        if (d.tool === "pen") {
          if (Math.hypot(w.x - last.x, w.y - last.y) * this.k < 1.5) return;
          d.pts.push(w);
        } else d.pts[1] = w;
        this.drawDraft(d.tool, d.pts);
        return;
      }
      if (d.kind === "pan") {
        this.tx += dx;
        this.ty += dy;
        d.x = e.clientX;
        d.y = e.clientY;
        d.moved = true;
        this.applyView();
      } else if (d.kind === "move") {
        if (!d.moved && Math.abs(dx) + Math.abs(dy) < 3) return;
        d.moved = true;
        for (const [id, s] of d.start) {
          const n = this.node(id)!;
          n.x = Math.round(s.x + dx / this.k);
          n.y = Math.round(s.y + dy / this.k);
          const el = this.cards.get(id);
          if (el) {
            this.place(el, n);
            el.classList.add("is-dragging");
          }
        }
        this.renderEdges();
      } else if (d.kind === "resize") {
        const n = this.node(d.id)!;
        const [minW, minH] = isInk(n) ? [8, 8] : isShape(n) ? [40, 30] : n.widget ? [90, 50] : [140, 70];
        n.width = Math.max(minW, Math.round(d.w + dx / this.k));
        n.height = Math.max(minH, Math.round(d.h + dy / this.k));
        this.place(this.cards.get(d.id)!, n);
        this.renderEdges();
      } else if (d.kind === "connect") {
        const w = this.toWorld(e.clientX, e.clientY);
        const over = (document.elementFromPoint(e.clientX, e.clientY) as HTMLElement | null)?.closest<HTMLElement>(".card");
        const target = over && over.dataset.id !== d.from ? over.dataset.id! : null;
        if (target !== d.target) {
          if (d.target) this.cards.get(d.target)?.classList.remove("is-target");
          if (target) this.cards.get(target)?.classList.add("is-target");
          d.target = target;
        }
        this.renderEdges({ from: this.node(d.from)!, x: w.x, y: w.y });
      }
    });
    const end = () => {
      const d = this.drag;
      this.drag = null;
      host.classList.remove("is-panning");
      if (!d) return;
      if (d.kind === "draw") {
        this.finishDrawing(d.tool, d.pts);
        return;
      }
      if (d.kind === "move") {
        for (const id of d.start.keys()) this.cards.get(id)?.classList.remove("is-dragging");
        if (d.moved) this.changed();
      } else if (d.kind === "resize") {
        this.changed();
      } else if (d.kind === "connect") {
        if (d.target) {
          this.cards.get(d.target)?.classList.remove("is-target");
          const a = this.node(d.from)!;
          const b = this.node(d.target)!;
          const exists = this.data.edges.some((x) => (x.fromNode === a.id && x.toNode === b.id) || (x.fromNode === b.id && x.toNode === a.id));
          if (!exists) {
            this.data.edges.push({ id: newId(), fromNode: a.id, fromSide: bestSide(a, b), toNode: b.id, toSide: bestSide(b, a) });
            this.changed();
          }
        }
        this.renderEdges();
      }
    };
    host.addEventListener("pointerup", end);
    host.addEventListener("pointercancel", end);
    host.addEventListener(
      "wheel",
      (e) => {
        if ((e.target as HTMLElement).closest("textarea, [data-scroll]") && !e.ctrlKey) return;
        // In a note, the wheel scrolls the page; Ctrl+wheel zooms the drawing.
        if (this.opts.inline && !(e.ctrlKey || e.metaKey)) return;
        e.preventDefault();
        if (e.ctrlKey || e.metaKey) {
          const r = host.getBoundingClientRect();
          this.zoomBy(Math.exp(-e.deltaY * 0.0025), e.clientX - r.left, e.clientY - r.top);
        } else {
          this.tx -= e.shiftKey ? e.deltaY : e.deltaX;
          this.ty -= e.shiftKey ? 0 : e.deltaY;
          this.applyView();
        }
      },
      { passive: false },
    );
    host.addEventListener("dblclick", (e) => {
      // Pointer capture retargets clicks to the host; look at what's really there.
      const t = (document.elementFromPoint(e.clientX, e.clientY) as HTMLElement | null) ?? (e.target as HTMLElement);
      if (t.closest(".canvas-tools, .canvas-zoom, input, textarea, [contenteditable='true'], [contenteditable='plaintext-only'], [data-live], .card-widget")) return;
      if (this.tool !== "select") return;
      const edgeId = (t.closest("[data-edge]") as HTMLElement | null)?.dataset.edge;
      if (edgeId) {
        this.editEdgeLabel(edgeId);
        return;
      }
      const cardEl = t.closest<HTMLElement>(".card");
      if (!cardEl) {
        this.addText(this.toWorld(e.clientX, e.clientY));
        return;
      }
      const n = this.node(cardEl.dataset.id!);
      if (n && isShape(n)) {
        this.select([n.id]);
        this.editLabel(n.id);
        return;
      }
      if (n?.type === "file" && n.file && /\.(md|canvas)$/i.test(n.file)) this.app.openPath(n.file, e.ctrlKey || e.metaKey);
    });
    host.addEventListener("keydown", (e) => {
      if ((e.target as HTMLElement).closest("textarea, input") || (e.target as HTMLElement).isContentEditable) {
        if (e.key === "Escape") host.focus({ preventScroll: true }); // keeps the card selected
        return;
      }
      const mod = e.ctrlKey || e.metaKey;
      const key = e.key.toLowerCase();
      if (e.key === "Delete" || e.key === "Backspace") {
        e.preventDefault();
        this.deleteSelection();
      } else if (e.key === "Escape") {
        if (this.tool !== "select") this.setTool("select");
        else this.select([]);
      } else if (mod && key === "z") {
        e.preventDefault();
        if (e.shiftKey) this.redo();
        else this.undo();
      } else if (mod && key === "y") {
        e.preventDefault();
        this.redo();
      } else if (mod && key === "a") {
        e.preventDefault();
        this.selectAll();
      } else if (e.key === "Enter" && this.selected.size === 1) {
        const n = this.node([...this.selected][0]);
        if (n && isShape(n)) {
          e.preventDefault();
          this.editLabel(n.id);
        }
      } else if (!mod && !e.altKey) {
        if (SHAPE_KEYS[key]) {
          e.preventDefault();
          this.setShape(SHAPE_KEYS[key]);
          return;
        }
        const t = TOOLS.find((x) => x[3].toLowerCase() === key);
        if (t) {
          e.preventDefault();
          this.setTool(t[0]);
        }
      }
    });
    host.addEventListener("dragover", (e) => {
      if (!e.dataTransfer?.types.includes(DRAG_TYPE)) return;
      e.preventDefault();
      e.dataTransfer.dropEffect = "copy";
    });
    host.addEventListener("drop", (e) => {
      const path = e.dataTransfer?.getData(DRAG_TYPE);
      if (!path) return;
      e.preventDefault();
      if (path === this.path) return;
      this.addFile(path, this.toWorld(e.clientX, e.clientY));
    });
  }
}

function parse(text: string): CData {
  if (!text.trim()) return { nodes: [], edges: [] };
  try {
    const d = JSON.parse(text) as Partial<CData>;
    const nodes = Array.isArray(d.nodes) ? d.nodes.filter((n) => n && typeof n.id === "string") : [];
    for (const n of nodes) {
      n.x = Number(n.x) || 0;
      n.y = Number(n.y) || 0;
      n.width = Number(n.width) || 280;
      n.height = Number(n.height) || 160;
    }
    return { ...d, nodes, edges: Array.isArray(d.edges) ? d.edges : [] };
  } catch {
    return { nodes: [], edges: [] };
  }
}

function anchor(n: CNode, side: Side) {
  switch (side) {
    case "top":
      return { x: n.x + n.width / 2, y: n.y };
    case "bottom":
      return { x: n.x + n.width / 2, y: n.y + n.height };
    case "left":
      return { x: n.x, y: n.y + n.height / 2 };
    default:
      return { x: n.x + n.width, y: n.y + n.height / 2 };
  }
}

/** The side of `a` that faces `b`. */
function bestSide(a: CNode, b: CNode): Side {
  const dx = b.x + b.width / 2 - (a.x + a.width / 2);
  const dy = b.y + b.height / 2 - (a.y + a.height / 2);
  if (Math.abs(dx) * 0.8 > Math.abs(dy)) return dx > 0 ? "right" : "left";
  return dy > 0 ? "bottom" : "top";
}

const NORMAL: Record<Side, { x: number; y: number }> = {
  top: { x: 0, y: -1 },
  bottom: { x: 0, y: 1 },
  left: { x: -1, y: 0 },
  right: { x: 1, y: 0 },
};

function curve(a: { x: number; y: number }, as: Side, b: { x: number; y: number }, bs: Side) {
  const dist = Math.hypot(b.x - a.x, b.y - a.y);
  const c = Math.max(30, Math.min(160, dist * 0.4));
  const na = NORMAL[as];
  const nb = NORMAL[bs];
  return `M${a.x},${a.y} C${a.x + na.x * c},${a.y + na.y * c} ${b.x + nb.x * c},${b.y + nb.y * c} ${b.x},${b.y}`;
}

const mid = (a: { x: number; y: number }, b: { x: number; y: number }) => ({ x: (a.x + b.x) / 2, y: (a.y + b.y) / 2 });

/** Drops points that barely change the line (Ramer–Douglas–Peucker). */
function simplify(pts: { x: number; y: number }[], eps: number): { x: number; y: number }[] {
  if (pts.length < 3) return pts;
  const [a, b] = [pts[0], pts[pts.length - 1]];
  const len = Math.hypot(b.x - a.x, b.y - a.y) || 1;
  let far = 0;
  let at = 0;
  for (let i = 1; i < pts.length - 1; i++) {
    const p = pts[i];
    const d = Math.abs((b.y - a.y) * p.x - (b.x - a.x) * p.y + b.x * a.y - b.y * a.x) / len;
    if (d > far) {
      far = d;
      at = i;
    }
  }
  if (far <= eps) return [a, b];
  return [...simplify(pts.slice(0, at + 1), eps).slice(0, -1), ...simplify(pts.slice(at), eps)];
}

/** A smooth line through the points (curves between midpoints). */
function smooth(pts: { x: number; y: number }[]): string {
  if (pts.length === 0) return "";
  if (pts.length < 3) return `M${pts[0].x},${pts[0].y} ` + pts.slice(1).map((p) => `L${p.x},${p.y}`).join(" ");
  let d = `M${pts[0].x},${pts[0].y}`;
  for (let i = 1; i < pts.length - 1; i++) {
    const m = mid(pts[i], pts[i + 1]);
    d += ` Q${pts[i].x},${pts[i].y} ${m.x},${m.y}`;
  }
  const last = pts[pts.length - 1];
  return d + ` L${last.x},${last.y}`;
}

/** A straight arrow from a to b, its head (and with `both`, its tail) drawn as two short lines. */
function arrowPath(a: { x: number; y: number }, b: { x: number; y: number }, size: number, both = false): string {
  const l = Math.max(12, size * 5);
  const head = (from: { x: number; y: number }, to: { x: number; y: number }) => {
    const ang = Math.atan2(to.y - from.y, to.x - from.x);
    const p = (t: number) => `${to.x - l * Math.cos(ang + t)},${to.y - l * Math.sin(ang + t)}`;
    return ` M${p(0.45)} L${to.x},${to.y} L${p(-0.45)}`;
  };
  return `M${a.x},${a.y} L${b.x},${b.y}` + head(a, b) + (both ? head(b, a) : "");
}

function inkPath(ink: Ink): string {
  const pts = ink.points
    .trim()
    .split(/\s+/)
    .map((pair) => pair.split(",").map(Number))
    .filter((p) => p.length === 2 && p.every(Number.isFinite))
    .map(([x, y]) => ({ x, y }));
  if (pts.length === 0) return "";
  if ((ink.arrow || ink.arrowStart) && pts.length >= 2) return arrowPath(pts[0], pts[pts.length - 1], ink.size, !!ink.arrowStart);
  return smooth(pts);
}

/** A shape, drawn small (for the picker). */
function shapeIcon(kind: string): SVGElement {
  const def = shapeDef(kind) ?? SHAPES[0];
  const svg = svgEl("svg", { class: "cv-shape-icon", viewBox: "-6 -6 112 112", "aria-hidden": "true" });
  svg.append(svgEl("path", { d: def.d, "vector-effect": "non-scaling-stroke" }));
  if (def.extra) svg.append(svgEl("path", { d: def.extra, fill: "none", "vector-effect": "non-scaling-stroke" }));
  return svg;
}

/** The same canvas, however its file is laid out. */
function sameBoard(a: string, b: string): boolean {
  if (a === b) return true;
  try {
    return JSON.stringify(JSON.parse(a)) === JSON.stringify(JSON.parse(b));
  } catch {
    return false;
  }
}
