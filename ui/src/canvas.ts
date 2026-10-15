// Canvas: a board of cards saved as a `.canvas` file in the JSON Canvas
// format (the one Obsidian uses), so boards open in either app.
//
// Drag a card to move it, drag its right-hand dot onto another card to
// connect them, drag the corner to resize. Drag the background (or scroll)
// to pan, Ctrl+scroll to zoom. Double-click the background for a text card,
// double-click a note card to open the note. Drop notes from the sidebar.

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
  [k: string]: unknown;
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
    this.world.append(this.edgesSvg);
    this.nameEl = h("div.name");
    this.zoomLabel = h("span");
    host.append(
      this.world,
      h(
        "div.canvas-over",
        null,
        h("div.smallcaps", null, "Canvas"),
        this.nameEl,
        h("div.canvas-hint", null, "Drag cards to arrange · double-click a note to open it · drop notes here from the sidebar"),
      ),
      h(
        "div.canvas-tools",
        null,
        h("button.btn.btn-secondary", { onclick: () => this.addText() }, icon("text-t"), "Add text card"),
      ),
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
    this.renderAll();
  }

  renamed(path: string) {
    this.path = path;
    this.nameEl.textContent = displayName(path);
  }

  private changed() {
    window.clearTimeout(this.saveTimer);
    this.saveTimer = window.setTimeout(() => void this.flush(), 600);
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
    if (n.type === "text") {
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
          .catch(() => {
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
    if (n.type !== "group") el.append(h("div.card-handle", { title: "Drag to another card to connect" }));
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
    for (const el of [...svg.querySelectorAll("path, text")]) el.remove();
    const mk = (d: string, cls: string, edge?: CEdge) => {
      const p = document.createElementNS(SVGNS, "path");
      p.setAttribute("d", d);
      if (cls) p.setAttribute("class", cls);
      if (edge) {
        const from = this.node(edge.fromNode);
        const to = this.node(edge.toNode);
        if (from?.type === "text" || to?.type === "text") p.setAttribute("stroke-dasharray", "5 5");
        if ((edge.toEnd ?? "arrow") === "arrow") p.setAttribute("marker-end", "url(#cv-arrow)");
        if (edge.fromEnd === "arrow") p.setAttribute("marker-start", "url(#cv-arrow)");
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
      hit.addEventListener("pointerdown", (ev) => {
        ev.stopPropagation();
        this.selectEdge(e.id);
      });
      if (e.label) {
        const m = mid(anchor(a, fs), anchor(b, ts));
        const t = document.createElementNS(SVGNS, "text");
        t.setAttribute("x", String(m.x));
        t.setAttribute("y", String(m.y - 6));
        t.setAttribute("text-anchor", "middle");
        t.setAttribute("style", "font-size:13px;fill:var(--color-text);paint-order:stroke;stroke:var(--color-bg);stroke-width:4px");
        t.textContent = e.label;
        svg.append(t);
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
  }

  private selectEdge(id: string) {
    this.selected.clear();
    for (const el of this.cards.values()) el.classList.remove("is-selected");
    this.selectedEdge = id;
    this.renderEdges();
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
      const cardEl = t.closest<HTMLElement>(".card");
      host.focus({ preventScroll: true });
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
        n.width = Math.max(140, Math.round(d.w + dx / this.k));
        n.height = Math.max(70, Math.round(d.h + dy / this.k));
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
      if (t.closest(".canvas-tools, .canvas-zoom")) return;
      const cardEl = t.closest<HTMLElement>(".card");
      if (!cardEl) {
        this.addText(this.toWorld(e.clientX, e.clientY));
        return;
      }
      const n = this.node(cardEl.dataset.id!);
      if (n?.type === "file" && n.file && /\.(md|canvas)$/i.test(n.file)) this.app.openPath(n.file, e.ctrlKey || e.metaKey);
    });
    host.addEventListener("keydown", (e) => {
      if ((e.target as HTMLElement).closest("textarea, input")) {
        if (e.key === "Escape") host.focus({ preventScroll: true }); // keeps the card selected
        return;
      }
      if (e.key === "Delete" || e.key === "Backspace") {
        e.preventDefault();
        this.deleteSelection();
      } else if (e.key === "Escape") {
        this.select([]);
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
