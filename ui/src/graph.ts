// Graph view renderer: notes as dots, links as lines, laid out by a live
// force simulation and drawn on a canvas so large vaults stay smooth.
//
// It animates: dots drift a little, signals travel along links in the
// direction they point, and the selected dot breathes. Hover traces a dot's
// neighbours; click selects it; double-click opens it. Drag the background
// to pan, scroll to zoom, drag a dot to move it.

import {
  forceCollide,
  forceLink,
  forceManyBody,
  forceSimulation,
  forceX,
  forceY,
  type Simulation,
  type SimulationLinkDatum,
  type SimulationNodeDatum,
} from "d3-force";

import type { GraphData, GraphNode } from "./api";

export interface Node extends SimulationNodeDatum, GraphNode {
  r: number;
  /** Phase for the idle drift, so dots don't move in step. */
  phase: number;
  color: string;
}
type Link = SimulationLinkDatum<Node> & { source: Node | string; target: Node | string };

export interface Forces {
  /** 0–1: how strongly dots are pulled to the middle. */
  gravity: number;
  /** How hard dots push each other apart. */
  repulsion: number;
  /** Rest length of a link, in px. */
  distance: number;
}

export const DEFAULT_FORCES: Forces = { gravity: 0.4, repulsion: 420, distance: 110 };

export interface GraphViewOptions {
  onSelect: (node: Node | null) => void;
  onOpen: (node: GraphNode, inSplit: boolean) => void;
  /** The view moved or zoomed (for the minimap). */
  onView?: () => void;
}

interface Palette {
  ink: string;
  paper: string;
  accent: string;
  tag: string;
  tagText: string;
  serif: string;
  dark: boolean;
}

const reducedMotion = matchMedia("(prefers-reduced-motion: reduce)");

export class GraphView {
  private canvas: HTMLCanvasElement;
  private ctx: CanvasRenderingContext2D;
  private sim: Simulation<Node, Link> | null = null;
  nodes: Node[] = [];
  links: Link[] = [];
  private byId = new Map<string, Node>();
  private neighbours = new Map<string, Set<string>>();
  private current: string | null = null;
  private hover: Node | null = null;
  selected: Node | null = null;
  k = 1;
  tx = 0;
  ty = 0;
  private dpr = 1;
  private frame = 0;
  private palette!: Palette;
  private drag: { node: Node | null; x: number; y: number; moved: boolean } | null = null;
  private resize: ResizeObserver;
  private autoFit = false;
  private labelled = new Set<string>();
  private forces: Forces = { ...DEFAULT_FORCES };
  private filter = "";
  private group: string | null = null;
  arrows = true;
  motion = !reducedMotion.matches;
  private t0 = performance.now();
  private destroyed = false;
  /** A view to glide towards (Fit, centring on a dot). */
  private glide: { k: number; tx: number; ty: number } | null = null;

  constructor(
    private host: HTMLElement,
    private opts: GraphViewOptions,
  ) {
    this.canvas = document.createElement("canvas");
    this.canvas.setAttribute("role", "img");
    this.canvas.setAttribute("aria-label", "Graph of notes and the links between them");
    host.append(this.canvas);
    this.ctx = this.canvas.getContext("2d")!;
    this.readPalette();
    this.resize = new ResizeObserver(() => this.fitCanvas());
    this.resize.observe(host);
    this.bindPointer();
    this.loop();
  }

  private readPalette() {
    const cs = getComputedStyle(document.documentElement);
    const v = (n: string) => cs.getPropertyValue(n).trim();
    this.palette = {
      ink: v("--color-text"),
      paper: v("--color-bg"),
      accent: v("--color-accent"),
      tag: v("--color-accent-2"),
      tagText: v("--color-accent-2-700"),
      serif: v("--font-body"),
      dark: isDark(v("--color-bg")),
    };
  }

  /** The theme changed. */
  restyle() {
    this.readPalette();
  }

  get isDark() {
    return this.palette.dark;
  }

  /** Replaces the data, keeping positions of dots that were already there. */
  setData(data: GraphData, current: string | null, colorOf: (n: GraphNode) => string) {
    this.current = current;
    const old = this.byId;
    this.byId = new Map();
    this.nodes = data.nodes.map((n) => {
      const prev = old.get(n.id);
      const node: Node = {
        ...n,
        r: n.kind === "tag" ? Math.min(12, 4 + Math.sqrt(n.degree) * 1.6) : Math.min(16, 4.5 + Math.sqrt(n.degree) * 2.4),
        phase: hash(n.id),
        color: colorOf(n),
        x: prev?.x,
        y: prev?.y,
        vx: prev?.vx,
        vy: prev?.vy,
      };
      this.byId.set(n.id, node);
      return node;
    });
    this.neighbours = new Map(this.nodes.map((n) => [n.id, new Set<string>()]));
    this.links = [];
    for (const e of data.edges) {
      if (!this.byId.has(e.source) || !this.byId.has(e.target)) continue;
      this.links.push({ source: e.source, target: e.target });
      this.neighbours.get(e.source)!.add(e.target);
      this.neighbours.get(e.target)!.add(e.source);
    }
    // New dots start beside a neighbour instead of flying in from the edge.
    for (const n of this.nodes) {
      if (n.x !== undefined) continue;
      const near = [...(this.neighbours.get(n.id) ?? [])].map((id) => this.byId.get(id)).find((m) => m?.x !== undefined);
      if (near) {
        n.x = near.x! + (Math.random() - 0.5) * 40;
        n.y = near.y! + (Math.random() - 0.5) * 40;
      }
    }
    if (this.selected) this.selected = this.byId.get(this.selected.id) ?? null;
    if (this.hover) this.hover = this.byId.get(this.hover.id) ?? null;

    const fresh = old.size === 0;
    if (!fresh && (this.nodes.length !== old.size || this.nodes.some((n) => !old.has(n.id)))) this.autoFit = true;
    this.sim?.stop();
    this.sim = forceSimulation<Node, Link>(this.nodes)
      .alphaDecay(0.025)
      .on("tick", () => {
        if (this.autoFit && this.sim!.alpha() < 0.1) {
          this.autoFit = false;
          this.fitToNodes();
        }
      });
    this.applyForces();
    this.sim.alpha(fresh ? 1 : 0.4);
    if (fresh) {
      this.centre();
      this.autoFit = true;
    }
    this.labelled = new Set(
      [...this.nodes]
        .sort((a, b) => b.degree - a.degree)
        .slice(0, Math.max(6, Math.ceil(this.nodes.length * 0.12)))
        .filter((n) => n.degree > 0)
        .map((n) => n.id),
    );
    this.opts.onSelect(this.selected);
  }

  setForces(f: Forces) {
    this.forces = { ...f };
    this.applyForces();
    this.sim?.alpha(0.5).restart();
  }

  private applyForces() {
    if (!this.sim) return;
    const f = this.forces;
    const big = this.nodes.length > 400;
    this.sim
      .force(
        "link",
        forceLink<Node, Link>(this.links)
          .id((d) => d.id)
          .distance(big ? f.distance * 0.7 : f.distance)
          .strength(0.4),
      )
      .force("charge", forceManyBody<Node>().strength(-(big ? f.repulsion * 0.6 : f.repulsion)).distanceMax(1000))
      .force("x", forceX<Node>(0).strength(0.01 + f.gravity * 0.12))
      .force("y", forceY<Node>(0).strength(0.015 + f.gravity * 0.16))
      .force("collide", forceCollide<Node>((d) => d.r + (big ? 6 : 24)));
  }

  /** Dims dots whose name doesn't contain `text`. */
  setFilter(text: string) {
    this.filter = text.trim().toLowerCase();
  }

  /** Dims dots outside one colour group (from the legend). */
  setGroup(color: string | null) {
    this.group = color;
  }

  get groupFilter() {
    return this.group;
  }

  select(id: string | null, centre = false) {
    this.selected = id ? this.byId.get(id) ?? null : null;
    this.opts.onSelect(this.selected);
    if (centre && this.selected) this.centreOn(this.selected);
  }

  node(id: string): Node | undefined {
    return this.byId.get(id);
  }

  neighboursOf(id: string): Node[] {
    return [...(this.neighbours.get(id) ?? [])].map((n) => this.byId.get(n)!).filter(Boolean);
  }

  /** Links into and out of a dot. */
  degreeOf(id: string): { incoming: number; outgoing: number } {
    let incoming = 0;
    let outgoing = 0;
    for (const l of this.links) {
      if ((l.source as Node).id === id) outgoing++;
      if ((l.target as Node).id === id) incoming++;
    }
    return { incoming, outgoing };
  }

  destroy() {
    this.destroyed = true;
    this.sim?.stop();
    this.resize.disconnect();
    cancelAnimationFrame(this.frame);
    this.canvas.remove();
  }

  // ------------------------------------------------------------ the view

  private fitCanvas() {
    const r = this.host.getBoundingClientRect();
    this.dpr = window.devicePixelRatio || 1;
    this.canvas.width = Math.max(1, Math.round(r.width * this.dpr));
    this.canvas.height = Math.max(1, Math.round(r.height * this.dpr));
    this.canvas.style.width = `${r.width}px`;
    this.canvas.style.height = `${r.height}px`;
  }

  get size() {
    return { w: this.canvas.width / this.dpr, h: this.canvas.height / this.dpr };
  }

  private centre() {
    const { w, h } = this.size;
    this.tx = w / 2;
    this.ty = h / 2;
    this.k = this.nodes.length > 300 ? 0.55 : this.nodes.length > 80 ? 0.8 : 1;
  }

  /** The box around every dot, in graph coordinates. */
  bounds() {
    let x0 = Infinity,
      y0 = Infinity,
      x1 = -Infinity,
      y1 = -Infinity;
    for (const n of this.nodes) {
      if (n.x === undefined) continue;
      x0 = Math.min(x0, n.x - n.r);
      y0 = Math.min(y0, n.y! - n.r);
      x1 = Math.max(x1, n.x + n.r);
      y1 = Math.max(y1, n.y! + n.r);
    }
    return x0 === Infinity ? null : { x0, y0, x1, y1 };
  }

  /** Zooms and pans so every dot fits, gliding there. */
  fitToNodes(instant = false) {
    const b = this.bounds();
    if (!b) return;
    const { w, h } = this.size;
    const padX = 90;
    const padTop = 150;
    const padBottom = 90;
    const k = Math.min((w - padX * 2) / Math.max(b.x1 - b.x0, 1), (h - padTop - padBottom) / Math.max(b.y1 - b.y0, 1));
    const kk = Math.min(1.6, Math.max(0.12, k));
    const target = {
      k: kk,
      tx: w / 2 - ((b.x0 + b.x1) / 2) * kk,
      ty: padTop + (h - padTop - padBottom) / 2 - ((b.y0 + b.y1) / 2) * kk,
    };
    if (instant || !this.motion) {
      Object.assign(this, target);
      this.opts.onView?.();
    } else this.glide = target;
  }

  centreOn(n: Node) {
    const { w, h } = this.size;
    const k = Math.max(this.k, 0.9);
    const target = { k, tx: w / 2 - n.x! * k, ty: h / 2 - n.y! * k };
    if (this.motion) this.glide = target;
    else {
      Object.assign(this, target);
      this.opts.onView?.();
    }
  }

  zoomBy(f: number, cx?: number, cy?: number) {
    const { w, h } = this.size;
    const mx = cx ?? w / 2;
    const my = cy ?? h / 2;
    const k = Math.min(4, Math.max(0.1, this.k * f));
    this.tx = mx - ((mx - this.tx) * k) / this.k;
    this.ty = my - ((my - this.ty) * k) / this.k;
    this.k = k;
    this.glide = null;
    this.autoFit = false;
    this.opts.onView?.();
  }

  /** Moves the view so graph point (x, y) is in the middle. */
  panTo(x: number, y: number) {
    const { w, h } = this.size;
    this.tx = w / 2 - x * this.k;
    this.ty = h / 2 - y * this.k;
    this.glide = null;
    this.autoFit = false;
    this.opts.onView?.();
  }

  // ------------------------------------------------------------ drawing

  private loop() {
    const step = () => {
      if (this.destroyed) return;
      this.frame = requestAnimationFrame(step);
      // Nothing to do while hidden (another tab is showing).
      if (document.hidden || this.host.offsetParent === null) return;
      this.tickGlide();
      this.draw();
    };
    this.frame = requestAnimationFrame(step);
  }

  private tickGlide() {
    const g = this.glide;
    if (!g) return;
    const a = 0.16;
    this.k += (g.k - this.k) * a;
    this.tx += (g.tx - this.tx) * a;
    this.ty += (g.ty - this.ty) * a;
    if (Math.abs(g.k - this.k) < 0.001 && Math.abs(g.tx - this.tx) < 0.5 && Math.abs(g.ty - this.ty) < 0.5) {
      Object.assign(this, g);
      this.glide = null;
    }
    this.opts.onView?.();
  }

  /** Where a dot is drawn this frame: its place plus a little drift. */
  private at(n: Node, t: number) {
    if (!this.motion || n === this.drag?.node) return { x: n.x!, y: n.y! };
    return { x: n.x! + Math.sin(t * 0.55 + n.phase * 6.28) * 2.4, y: n.y! + Math.cos(t * 0.45 + n.phase * 5.1) * 2.4 };
  }

  private draw() {
    const { ctx, palette: p, k } = this;
    const { w, h } = this.size;
    const t = (performance.now() - this.t0) / 1000;
    ctx.setTransform(this.dpr, 0, 0, this.dpr, 0, 0);
    ctx.clearRect(0, 0, w, h);
    this.drawGrid(w, h);
    ctx.translate(this.tx, this.ty);
    ctx.scale(k, k);

    const focus = this.hover ?? this.selected;
    const lit = focus ? this.neighbours.get(focus.id) ?? new Set<string>() : null;
    const matches = (n: Node) => (!this.filter || n.name.toLowerCase().includes(this.filter)) && (!this.group || n.color === this.group);
    const dimAll = !!this.filter || !!this.group;
    const pos = new Map<Node, { x: number; y: number }>();
    for (const n of this.nodes) if (n.x !== undefined) pos.set(n, this.at(n, t));

    // Links, then the signals running along them.
    const many = this.links.length > 500;
    for (const l of this.links) {
      const s = l.source as Node;
      const d = l.target as Node;
      const a = pos.get(s);
      const b = pos.get(d);
      if (!a || !b) continue;
      const on = focus !== null && (s === focus || d === focus);
      const faded = (focus !== null && !on) || (dimAll && !(matches(s) && matches(d)));
      const color = on ? (s === focus ? s.color : d.color) : p.ink;
      ctx.globalAlpha = on ? 0.85 : faded ? 0.06 : p.dark ? 0.28 : 0.2;
      ctx.strokeStyle = color;
      ctx.lineWidth = (on ? 1.8 : 1) / Math.max(k, 0.5);
      ctx.beginPath();
      ctx.moveTo(a.x, a.y);
      ctx.lineTo(b.x, b.y);
      ctx.stroke();
      if (this.arrows && !faded && (on || k > 0.7)) this.arrowHead(a, b, d.r, on ? 0.85 : 0.3, color);

      // A signal travels source → target at about 70 px a second.
      if (this.motion && !faded && (!many || on)) {
        const len = Math.hypot(b.x - a.x, b.y - a.y) || 1;
        const prog = ((t * 70) / len + s.phase + d.phase) % 1;
        const fade = Math.sin(prog * Math.PI); // fades in and out at the ends
        ctx.globalAlpha = (on ? 0.95 : 0.55) * fade;
        ctx.fillStyle = s.color;
        ctx.beginPath();
        ctx.arc(a.x + (b.x - a.x) * prog, a.y + (b.y - a.y) * prog, (on ? 2.6 : 1.8) / Math.max(k, 0.6), 0, Math.PI * 2);
        ctx.fill();
      }
    }

    // Dots
    for (const n of this.nodes) {
      const q = pos.get(n);
      if (!q) continue;
      const isFocus = n === focus;
      const isSel = n === this.selected;
      const isNear = lit?.has(n.id) ?? false;
      const isCurrent = n.id === this.current;
      const faded = (focus !== null && !isFocus && !isNear) || (dimAll && !matches(n));

      // A breathing halo on the selected, hovered and current dots.
      if (!faded && (isFocus || isSel || isCurrent)) {
        const breathe = this.motion ? Math.sin(t * 2.6) * 2 : 0;
        ctx.globalAlpha = isFocus || isSel ? 0.22 : 0.14;
        ctx.fillStyle = isCurrent && !isSel && !isFocus ? p.accent : n.color;
        ctx.beginPath();
        ctx.arc(q.x, q.y, n.r + 7 + breathe, 0, Math.PI * 2);
        ctx.fill();
      }

      ctx.globalAlpha = faded ? 0.15 : 1;
      ctx.beginPath();
      ctx.arc(q.x, q.y, n.r, 0, Math.PI * 2);
      if (n.kind === "unresolved") {
        ctx.fillStyle = p.paper;
        ctx.fill();
        ctx.setLineDash([2.5 / k, 2.5 / k]);
        ctx.strokeStyle = p.tag;
        ctx.lineWidth = 1.5 / k;
        ctx.stroke();
        ctx.setLineDash([]);
        continue;
      }
      ctx.fillStyle = n.color;
      ctx.shadowColor = p.dark ? "rgba(0,0,0,0.6)" : "rgba(32,30,29,0.22)";
      ctx.shadowBlur = isFocus ? 10 : 4;
      ctx.shadowOffsetY = 1;
      ctx.fill();
      ctx.shadowBlur = 0;
      ctx.shadowOffsetY = 0;
      // A paper rim and core, like a printed bead.
      ctx.strokeStyle = p.paper;
      ctx.lineWidth = Math.min(2, n.r * 0.3);
      ctx.stroke();
      if (n.r >= 7 && n.kind !== "tag") {
        ctx.beginPath();
        ctx.arc(q.x, q.y, n.r * 0.3, 0, Math.PI * 2);
        ctx.fillStyle = p.paper;
        ctx.fill();
      }
      if (isCurrent) {
        ctx.strokeStyle = p.accent;
        ctx.lineWidth = 2 / k;
        ctx.beginPath();
        ctx.arc(q.x, q.y, n.r + 4 / k, 0, Math.PI * 2);
        ctx.stroke();
      }
    }

    // Labels on soft paper badges.
    const showAll = k >= 1.4 || (this.nodes.length <= 80 && k >= 0.55);
    const ks = Math.max(k, 0.75);
    const size = 13 / ks;
    ctx.textAlign = "center";
    ctx.textBaseline = "middle";
    for (const n of this.nodes) {
      const q = pos.get(n);
      if (!q) continue;
      const special = n === focus || n === this.selected || n.id === this.current;
      const near = lit?.has(n.id) ?? false;
      const match = dimAll && matches(n);
      if (!showAll && !special && !near && !match && !(this.labelled.has(n.id) && !focus)) continue;
      const faded = (focus !== null && !special && !near) || (dimAll && !matches(n));
      const label = n.name.length > 28 ? n.name.slice(0, 27) + "…" : n.name;
      ctx.font = `${special ? 600 : 400} ${size}px ${p.serif}`;
      const bw = ctx.measureText(label).width + 12 / ks;
      const bh = size + 7 / ks;
      const by = q.y + n.r + 4 / k + bh / 2;
      ctx.globalAlpha = (faded ? 0.12 : 1) * (special ? 0.97 : 0.8);
      ctx.fillStyle = p.paper;
      roundRect(ctx, q.x - bw / 2, by - bh / 2, bw, bh, 4 / ks);
      ctx.fill();
      if (special) {
        ctx.globalAlpha = 1;
        ctx.strokeStyle = n.color;
        ctx.lineWidth = 1 / k;
        ctx.stroke();
      }
      ctx.globalAlpha = faded ? 0.12 : special ? 1 : 0.85;
      ctx.fillStyle = n.kind === "tag" || n.kind === "unresolved" ? p.tagText : p.ink;
      ctx.fillText(label, q.x, by + 0.5);
    }
    ctx.globalAlpha = 1;
  }

  private arrowHead(a: { x: number; y: number }, b: { x: number; y: number }, r: number, alpha: number, color: string) {
    const { ctx, k } = this;
    const ang = Math.atan2(b.y - a.y, b.x - a.x);
    const tipX = b.x - Math.cos(ang) * (r + 2);
    const tipY = b.y - Math.sin(ang) * (r + 2);
    const s = 6 / Math.max(k, 0.6);
    ctx.globalAlpha = alpha;
    ctx.fillStyle = color;
    ctx.beginPath();
    ctx.moveTo(tipX, tipY);
    ctx.lineTo(tipX - Math.cos(ang - 0.42) * s, tipY - Math.sin(ang - 0.42) * s);
    ctx.lineTo(tipX - Math.cos(ang + 0.42) * s, tipY - Math.sin(ang + 0.42) * s);
    ctx.closePath();
    ctx.fill();
  }

  /** A faint grid that moves with the graph, like ruled paper. */
  private drawGrid(w: number, h: number) {
    const { ctx, k, tx, ty, palette: p } = this;
    let step = 80 * k;
    while (step < 28) step *= 2;
    ctx.globalAlpha = p.dark ? 0.07 : 0.06;
    ctx.strokeStyle = p.ink;
    ctx.lineWidth = 1;
    ctx.beginPath();
    for (let x = ((tx % step) + step) % step; x < w; x += step) {
      ctx.moveTo(Math.round(x) + 0.5, 0);
      ctx.lineTo(Math.round(x) + 0.5, h);
    }
    for (let y = ((ty % step) + step) % step; y < h; y += step) {
      ctx.moveTo(0, Math.round(y) + 0.5);
      ctx.lineTo(w, Math.round(y) + 0.5);
    }
    ctx.stroke();
    ctx.globalAlpha = 1;
  }

  // ---------------------------------------------------------- interaction

  private toWorld(cx: number, cy: number) {
    const r = this.canvas.getBoundingClientRect();
    return { x: (cx - r.left - this.tx) / this.k, y: (cy - r.top - this.ty) / this.k };
  }

  private hit(cx: number, cy: number): Node | null {
    const { x, y } = this.toWorld(cx, cy);
    let best: Node | null = null;
    let bestD = Infinity;
    for (const n of this.nodes) {
      if (n.x === undefined) continue;
      const d = Math.hypot(n.x - x, n.y! - y);
      if (d < n.r + 6 / this.k && d < bestD) {
        best = n;
        bestD = d;
      }
    }
    return best;
  }

  private bindPointer() {
    const c = this.canvas;
    c.addEventListener("pointerdown", (e) => {
      this.autoFit = false;
      this.glide = null;
      c.setPointerCapture(e.pointerId);
      const node = this.hit(e.clientX, e.clientY);
      this.drag = { node, x: e.clientX, y: e.clientY, moved: false };
      if (node) {
        node.fx = node.x;
        node.fy = node.y;
        this.sim?.alphaTarget(0.2).restart();
      }
    });
    c.addEventListener("pointermove", (e) => {
      if (!this.drag) {
        const hit = this.hit(e.clientX, e.clientY);
        if (hit !== this.hover) {
          this.hover = hit;
          c.style.cursor = hit ? "pointer" : "grab";
        }
        return;
      }
      const dx = e.clientX - this.drag.x;
      const dy = e.clientY - this.drag.y;
      if (Math.abs(dx) + Math.abs(dy) > 3) this.drag.moved = true;
      if (this.drag.node) {
        const w = this.toWorld(e.clientX, e.clientY);
        this.drag.node.fx = w.x;
        this.drag.node.fy = w.y;
      } else {
        this.tx += dx;
        this.ty += dy;
        this.drag.x = e.clientX;
        this.drag.y = e.clientY;
        c.style.cursor = "grabbing";
        this.opts.onView?.();
      }
    });
    const end = (e: PointerEvent) => {
      const d = this.drag;
      this.drag = null;
      c.style.cursor = this.hover ? "pointer" : "grab";
      if (!d) return;
      if (d.node) {
        d.node.fx = null;
        d.node.fy = null;
        this.sim?.alphaTarget(0);
        if (!d.moved && e.type === "pointerup") {
          if (e.ctrlKey || e.metaKey) this.opts.onOpen(d.node, true);
          else this.select(d.node.id);
        }
      } else if (!d.moved && e.type === "pointerup") {
        this.select(null);
      }
    };
    c.addEventListener("pointerup", end);
    c.addEventListener("pointercancel", end);
    c.addEventListener("dblclick", (e) => {
      const n = this.hit(e.clientX, e.clientY);
      if (n) this.opts.onOpen(n, e.ctrlKey || e.metaKey);
    });
    c.addEventListener("pointerleave", () => {
      if (!this.drag) this.hover = null;
    });
    c.addEventListener(
      "wheel",
      (e) => {
        e.preventDefault();
        const r = c.getBoundingClientRect();
        this.zoomBy(Math.exp(-e.deltaY * 0.0015), e.clientX - r.left, e.clientY - r.top);
      },
      { passive: false },
    );
    c.style.cursor = "grab";
  }
}

function roundRect(ctx: CanvasRenderingContext2D, x: number, y: number, w: number, h: number, r: number) {
  ctx.beginPath();
  ctx.moveTo(x + r, y);
  ctx.arcTo(x + w, y, x + w, y + h, r);
  ctx.arcTo(x + w, y + h, x, y + h, r);
  ctx.arcTo(x, y + h, x, y, r);
  ctx.arcTo(x, y, x + w, y, r);
  ctx.closePath();
}

/** A stable number in [0, 1) for a string. */
function hash(s: string): number {
  let h = 2166136261;
  for (let i = 0; i < s.length; i++) h = Math.imul(h ^ s.charCodeAt(i), 16777619);
  return ((h >>> 0) % 10000) / 10000;
}

function isDark(color: string): boolean {
  const m = /^#?([0-9a-f]{2})([0-9a-f]{2})([0-9a-f]{2})/i.exec(color.trim());
  if (!m) return false;
  const [r, g, b] = m.slice(1).map((x) => parseInt(x, 16));
  return 0.299 * r + 0.587 * g + 0.114 * b < 128;
}

/** Keeps only dots within `depth` links of `center` (in either direction). */
export function localSubgraph(data: GraphData, center: string, depth: number): GraphData {
  const adj = new Map<string, string[]>();
  for (const e of data.edges) {
    (adj.get(e.source) ?? adj.set(e.source, []).get(e.source)!).push(e.target);
    (adj.get(e.target) ?? adj.set(e.target, []).get(e.target)!).push(e.source);
  }
  const keep = new Set([center]);
  let frontier = [center];
  for (let i = 0; i < depth; i++) {
    const next: string[] = [];
    for (const id of frontier) for (const n of adj.get(id) ?? []) if (!keep.has(n)) keep.add(n), next.push(n);
    frontier = next;
  }
  return {
    nodes: data.nodes.filter((n) => keep.has(n.id)),
    edges: data.edges.filter((e) => keep.has(e.source) && keep.has(e.target)),
  };
}
