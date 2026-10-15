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
import { openUrl } from "@tauri-apps/plugin-opener";

import { api, displayName, parentOf } from "./api";
import { DRAG_TYPE } from "./tree";
import { h, icon, plainText, trunc } from "./util";

type Side = "top" | "right" | "bottom" | "left";

interface CNode {
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
  /** Marrow: draw this text card as a shape. */
  shape?: ShapeKind;
  /** Marrow: a pen stroke or a free arrow. */
  ink?: Ink;
  [k: string]: unknown;
}

type ShapeKind = "rect" | "ellipse" | "diamond";
type Tool = "select" | "text" | ShapeKind | "arrow" | "pen";

interface Ink {
  /** The box the points were drawn in (the card scales it). */
  w: number;
  h: number;
  /** Line width. */
  size: number;
  /** "x,y x,y …" inside that box. */
  points: string;
  arrow?: boolean;
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

interface CData {
  nodes: CNode[];
  edges: CEdge[];
  [k: string]: unknown;
}

export interface CanvasApp {
  vaultRoot(): string;
  openPath(path: string, inSplit: boolean): void;
  toast(text: string, kind?: "info" | "error"): void;
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
const isShape = (n: CNode) => n.type === "text" && (n.shape === "rect" || n.shape === "ellipse" || n.shape === "diamond");
const svgEl = (tag: string, attrs: Record<string, string | number>) => {
  const el = document.createElementNS(SVGNS, tag);
  for (const [k, v] of Object.entries(attrs)) el.setAttribute(k, String(v));
  return el;
};
const TOOLS: [Tool, string, string, string][] = [
  ["select", "cursor", "Select and move", "V"],
  ["text", "text-t", "Text card", "T"],
  ["rect", "square", "Rectangle", "R"],
  ["ellipse", "circle", "Ellipse", "O"],
  ["diamond", "diamond", "Diamond", "D"],
  ["arrow", "arrow-up-right", "Arrow", "A"],
  ["pen", "pencil-simple", "Pen", "P"],
];
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
    | { kind: "draw"; tool: ShapeKind | "arrow" | "pen"; pts: { x: number; y: number }[]; x: number; y: number }
    | null = null;

  constructor(
    private host: HTMLElement,
    private app: CanvasApp,
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
    host.append(
      this.world,
      h(
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
    if (this.path && this.path !== path) {
      await this.flush();
      this.views.set(this.path, { k: this.k, tx: this.tx, ty: this.ty });
    }
    const text = await api.readNote(path);
    this.path = path;
    this.lastSaved = text;
    this.data = parse(text);
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
  }

  private scheduleSave() {
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
  }

  async flush() {
    window.clearTimeout(this.saveTimer);
    this.saveTimer = undefined;
    if (this.saving) await this.saving;
    if (!this.path) return;
    const text = JSON.stringify(this.data, null, "\t");
    if (text === this.lastSaved) return;
    const path = this.path;
    this.saving = api
      .writeNote(path, text)
      .then(() => {
        this.lastSaved = text;
      })
      .catch((e) => this.app.toast(`Couldn't save the canvas: ${e}`, "error"))
      .finally(() => {
        this.saving = null;
      });
    await this.saving;
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
    if (isInk(n)) {
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
    if (n.type !== "group" && !isInk(n)) el.append(h("div.card-handle", { title: "Drag to another card to connect" }));
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

  /** A rectangle, ellipse or diamond with a label (double-click to edit it). */
  private shapeCard(n: CNode): HTMLElement {
    const svg = svgEl("svg", { class: "shape-svg", viewBox: "0 0 100 100", preserveAspectRatio: "none" });
    const attrs = { class: "shape-fill", "vector-effect": "non-scaling-stroke" };
    svg.append(
      n.shape === "ellipse"
        ? svgEl("ellipse", { ...attrs, cx: 50, cy: 50, rx: 49, ry: 49 })
        : n.shape === "diamond"
          ? svgEl("polygon", { ...attrs, points: "50,1 99,50 50,99 1,50" })
          : svgEl("rect", { ...attrs, x: 1, y: 1, width: 98, height: 98, rx: 4, ry: 4 }),
    );
    const label = h("div.shape-label", { spellcheck: false });
    label.textContent = n.text ?? "";
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
    return h("div.card.card-shape", { dataset: { shape: n.shape ?? "rect" } }, svg, label);
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
    svg.append(svgEl("path", { class: "ink-line", d, "stroke-width": ink.size }), svgEl("path", { class: "ink-hit", d }));
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
    const padX = 60, padTop = 130, padBottom = 80;
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
      h("button.cv-tool", { type: "button", title: `${label} (${key})`, dataset: { tool: t }, onclick: () => this.setTool(t) }, icon(ic)),
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
      h("div.cv-bar", null, ...swatches, h("span.cv-pen-sizes", null, h("span.cv-sep"), ...sizes)),
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
    this.toolbar.classList.toggle("is-pen", this.tool === "pen");
    for (const t of TOOLS) this.host.classList.toggle(`tool-${t[0]}`, t[0] === this.tool);
  }

  setTool(t: Tool) {
    this.tool = t;
    this.syncToolbar();
  }

  private setPenSize(sz: number) {
    this.penSize = sz;
    this.setTool("pen");
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

  /** Finishes a shape, an arrow or a pen stroke. */
  private finishDrawing(tool: ShapeKind | "arrow" | "pen", pts: { x: number; y: number }[]) {
    this.draftG.replaceChildren();
    const a = pts[0];
    const b = pts[pts.length - 1];
    let n: CNode | null = null;
    if (tool === "pen" || tool === "arrow") {
      let ps = tool === "arrow" ? [a, b] : simplify(pts, 0.8 / this.k);
      if (tool === "arrow" && Math.hypot(b.x - a.x, b.y - a.y) < 8) return;
      if (ps.length === 1) ps = [ps[0], { x: ps[0].x + 0.5, y: ps[0].y + 0.5 }];
      const size = tool === "arrow" ? 2.5 : this.penSize;
      const pad = size + (tool === "arrow" ? 12 : 4);
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
          ...(tool === "arrow" ? { arrow: true } : {}),
        },
      };
    } else {
      const small = Math.abs(b.x - a.x) < 8 && Math.abs(b.y - a.y) < 8;
      const box = small
        ? { x: a.x - 80, y: a.y - 50, w: 160, h: 100 }
        : { x: Math.min(a.x, b.x), y: Math.min(a.y, b.y), w: Math.abs(b.x - a.x), h: Math.abs(b.y - a.y) };
      n = { id: newId(), type: "text", text: "", shape: tool, x: Math.round(box.x), y: Math.round(box.y), width: Math.max(40, Math.round(box.w)), height: Math.max(30, Math.round(box.h)) };
    }
    if (this.color) n.color = this.color;
    this.data.nodes.push(n);
    this.world.append(this.card(n));
    this.changed();
    if (tool === "pen") return; // keep drawing
    this.select([n.id]);
    this.setTool("select");
    if (isShape(n)) requestAnimationFrame(() => this.editLabel(n!.id));
  }

  private drawDraft(tool: ShapeKind | "arrow" | "pen", pts: { x: number; y: number }[]) {
    const a = pts[0];
    const b = pts[pts.length - 1];
    const c = colorOf(this.color) ?? "";
    let el: SVGElement;
    if (tool === "pen") {
      el = svgEl("path", { d: smooth(pts), class: "ink-draft", "stroke-width": this.penSize });
    } else if (tool === "arrow") {
      el = svgEl("path", { d: arrowPath(a, b, 2.5), class: "ink-draft", "stroke-width": 2.5 });
    } else {
      const x = Math.min(a.x, b.x), y = Math.min(a.y, b.y), w = Math.abs(b.x - a.x), hh = Math.abs(b.y - a.y);
      el =
        tool === "ellipse"
          ? svgEl("ellipse", { cx: x + w / 2, cy: y + hh / 2, rx: w / 2, ry: hh / 2, class: "shape-draft" })
          : tool === "diamond"
            ? svgEl("polygon", { points: `${x + w / 2},${y} ${x + w},${y + hh / 2} ${x + w / 2},${y + hh} ${x},${y + hh / 2}`, class: "shape-draft" })
            : svgEl("rect", { x, y, width: w, height: hh, rx: 4, class: "shape-draft" });
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
        const [minW, minH] = isInk(n) ? [8, 8] : isShape(n) ? [40, 30] : [140, 70];
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
        if ((e.target as HTMLElement).closest("textarea") && !e.ctrlKey) return;
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
      if (t.closest(".canvas-tools, .canvas-zoom, input, textarea, [contenteditable='true'], [contenteditable='plaintext-only']")) return;
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

/** A straight arrow from a to b, with its head drawn as two short lines. */
function arrowPath(a: { x: number; y: number }, b: { x: number; y: number }, size: number): string {
  const ang = Math.atan2(b.y - a.y, b.x - a.x);
  const l = Math.max(12, size * 5);
  const p = (t: number) => `${b.x - l * Math.cos(ang + t)},${b.y - l * Math.sin(ang + t)}`;
  return `M${a.x},${a.y} L${b.x},${b.y} M${p(0.45)} L${b.x},${b.y} L${p(-0.45)}`;
}

function inkPath(ink: Ink): string {
  const pts = ink.points
    .trim()
    .split(/\s+/)
    .map((pair) => pair.split(",").map(Number))
    .filter((p) => p.length === 2 && p.every(Number.isFinite))
    .map(([x, y]) => ({ x, y }));
  if (pts.length === 0) return "";
  return ink.arrow && pts.length >= 2 ? arrowPath(pts[0], pts[pts.length - 1], ink.size) : smooth(pts);
}
