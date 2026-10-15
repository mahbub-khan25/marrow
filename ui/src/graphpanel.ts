// Everything around the graph canvas: the title and colour legend, the
// filter / depth / toggle bars, the forces drawer, the minimap with zoom
// controls, and the card that describes the selected dot.

import { api, displayName, parentOf, type GraphData, type GraphNode } from "./api";
import { DEFAULT_FORCES, GraphView, localSubgraph, type Forces, type Node } from "./graph";
import { h, icon, kbd, plainText, storage, store, trunc } from "./util";

export interface GraphPanelApp {
  open(node: GraphNode, inSplit: boolean): void;
  /** The note most recently shown, used as the centre for Depth. */
  currentNote(): string | null;
}

interface GraphPrefs {
  showTags: boolean;
  showUnresolved: boolean;
  showAttachments: boolean;
  orphans: boolean;
  arrows: boolean;
  motion: boolean;
  colorBy: "folder" | "tag";
  /** 0 = whole vault; else steps from the open note. */
  depth: number;
  forces: Forces;
}

const DEFAULTS: GraphPrefs = {
  showTags: false,
  showUnresolved: false,
  showAttachments: false,
  orphans: true,
  arrows: true,
  motion: !matchMedia("(prefers-reduced-motion: reduce)").matches,
  colorBy: "folder",
  depth: 0,
  forces: { ...DEFAULT_FORCES },
};

/** Process inks: cyan, magenta, yellow, then companions in the same register. */
const INKS_LIGHT = ["#0088b0", "#d6006c", "#c48f00", "#2f8f5b", "#6b4fbf", "#d9622b", "#14837a", "#8a5a3c"];
const INKS_DARK = ["#38a6cf", "#ff5c98", "#edbb00", "#4cc38a", "#9b84f0", "#ff8a50", "#3cc0b0", "#c79a7a"];

interface Group {
  key: string;
  label: string;
  color: string;
  count: number;
}

export class GraphPanel {
  view: GraphView;
  private prefs: GraphPrefs;
  private data: GraphData = { nodes: [], edges: [] };
  private groups: Group[] = [];
  private seq = 0;
  private cardSeq = 0;

  private count = h("div.g-count");
  private legend = h("div.g-pill.g-legend");
  private filter = h("input.g-filter", { placeholder: "Filter nodes…", spellcheck: false, autocomplete: "off" }) as HTMLInputElement;
  private bars = h("div.g-bars");
  private drawer = h("div.g-card.g-drawer", { hidden: true });
  private card = h("div.g-card.g-node", { hidden: true });
  private mini = h("canvas.g-mini", { width: 352, height: 240 }) as HTMLCanvasElement;
  private zoomLabel = h("span.g-zoomval");
  private empty = h("p.graph-empty", { hidden: true }, "No notes to show yet. Link some notes with [[…]] and they’ll appear here.");
  private miniTimer = 0;

  constructor(
    private host: HTMLElement,
    private app: GraphPanelApp,
  ) {
    this.prefs = { ...DEFAULTS, ...storage<Partial<GraphPrefs>>("marrow.graph3", {}) };
    this.prefs.forces = { ...DEFAULT_FORCES, ...this.prefs.forces };
    const canvasHost = h("div.graph-canvas");
    host.replaceChildren(
      canvasHost,
      h("div.g-vignette"),
      h("div.g-top", null, h("div.g-title", null, h("div.smallcaps", null, "Graph view"), this.count, this.legend), this.bars),
      this.drawer,
      h(
        "div.g-bottomleft",
        null,
        h(
          "div.g-card.g-minicard",
          null,
          this.mini,
          h(
            "div.g-zoom",
            null,
            h("button", { title: "Zoom out", onclick: () => this.view.zoomBy(1 / 1.25) }, icon("minus")),
            this.zoomLabel,
            h("button", { title: "Zoom in", onclick: () => this.view.zoomBy(1.25) }, icon("plus")),
            h("button.g-fit", { title: "Fit everything in view", onclick: () => this.view.fitToNodes() }, "Fit"),
          ),
        ),
      ),
      h(
        "div.g-pill.g-hints",
        null,
        h("span", null, h("b", null, "Drag"), " pan"),
        "·",
        h("span", null, h("b", null, "Scroll"), " zoom"),
        "·",
        h("span", null, h("b", null, "Click"), " inspect"),
        "·",
        h("span", null, h("b", null, "Double-click"), " open"),
      ),
      this.card,
      this.empty,
    );
    this.view = new GraphView(canvasHost, {
      onSelect: (n) => void this.renderCard(n),
      onOpen: (n, split) => this.app.open(n, split),
      onView: () => this.scheduleMini(),
    });
    this.view.arrows = this.prefs.arrows;
    this.view.motion = this.prefs.motion;
    this.view.setForces(this.prefs.forces);

    this.filter.addEventListener("input", () => this.view.setFilter(this.filter.value));
    this.filter.addEventListener("keydown", (e) => {
      if (e.key === "Escape") {
        e.stopPropagation();
        this.filter.value = "";
        this.view.setFilter("");
      } else if (e.key === "Enter") {
        // Jump to the first match.
        const q = this.filter.value.trim().toLowerCase();
        const n = q && this.view.nodes.find((x) => x.name.toLowerCase().includes(q));
        if (n) this.view.select(n.id, true);
      }
    });
    this.bindMinimap();
    host.tabIndex = -1;
    host.addEventListener("keydown", (e) => {
      if ((e.target as HTMLElement).closest("input")) return;
      const sel = this.view.selected;
      if (e.key === "Enter" && sel) {
        e.preventDefault();
        this.app.open(sel, e.ctrlKey || e.metaKey);
      } else if (e.key === "Escape" && sel) {
        e.stopPropagation();
        this.view.select(null);
      } else if (e.key === "/" || (e.key.toLowerCase() === "f" && (e.ctrlKey || e.metaKey))) {
        e.preventDefault();
        this.filter.focus();
        this.filter.select();
      }
    });
    this.renderBars();
    window.setInterval(() => this.scheduleMini(), 250); // dots keep settling and drifting
  }

  private save() {
    store("marrow.graph3", this.prefs);
  }

  private set(patch: Partial<GraphPrefs>, reload = true) {
    Object.assign(this.prefs, patch);
    this.save();
    this.view.arrows = this.prefs.arrows;
    this.view.motion = this.prefs.motion;
    this.renderBars();
    if (reload) void this.refresh();
  }

  restyle() {
    this.view.restyle();
    this.recolor();
  }

  destroy() {
    this.view.destroy();
  }

  // ------------------------------------------------------------- data

  async refresh() {
    const mine = ++this.seq;
    const p = this.prefs;
    let data = await api.graph({ showTags: p.showTags, showUnresolved: p.showUnresolved, showAttachments: p.showAttachments }).catch(() => null);
    if (!data || mine !== this.seq) return;
    if (!p.orphans) {
      const linked = new Set<string>();
      for (const e of data.edges) linked.add(e.source), linked.add(e.target);
      data = { nodes: data.nodes.filter((n) => linked.has(n.id)), edges: data.edges };
    }
    const centre = this.app.currentNote();
    if (p.depth > 0 && centre && data.nodes.some((n) => n.id === centre)) data = localSubgraph(data, centre, p.depth);
    this.data = data;
    this.computeGroups();
    this.view.setData(data, centre, (n) => this.colorOf(n));
    const kinds = new Map(data.nodes.map((n) => [n.id, n.kind]));
    const notes = data.nodes.filter((n) => n.kind === "note").length;
    const links = data.edges.filter((e) => kinds.get(e.source) === "note" && kinds.get(e.target) === "note").length;
    this.count.textContent = `${notes} ${notes === 1 ? "note" : "notes"}, ${links} ${links === 1 ? "link" : "links"}`;
    this.empty.hidden = data.nodes.length > 0;
    this.renderLegend();
    this.renderDrawer();
  }

  /** Colour groups: top-level folder, or each note's most common tag. */
  private groupKey(n: GraphNode, tagFreq: Map<string, number>): string | null {
    if (n.kind !== "note") return null;
    if (this.prefs.colorBy === "tag") {
      if (!n.tags.length) return null;
      return [...n.tags].sort((a, b) => (tagFreq.get(b) ?? 0) - (tagFreq.get(a) ?? 0) || a.localeCompare(b))[0];
    }
    const i = n.id.indexOf("/");
    return i < 0 ? null : n.id.slice(0, i);
  }

  private tagFreq() {
    const f = new Map<string, number>();
    for (const n of this.data.nodes) for (const t of n.tags) f.set(t, (f.get(t) ?? 0) + 1);
    return f;
  }

  private computeGroups() {
    const freq = this.tagFreq();
    const counts = new Map<string, number>();
    for (const n of this.data.nodes) {
      const k = this.groupKey(n, freq);
      if (k) counts.set(k, (counts.get(k) ?? 0) + 1);
    }
    const inks = this.view.isDark ? INKS_DARK : INKS_LIGHT;
    this.groups = [...counts.entries()]
      .sort((a, b) => b[1] - a[1] || a[0].localeCompare(b[0]))
      .map(([key, count], i) => ({
        key,
        label: key,
        count,
        // Past the eighth group, colours repeat at lower contrast.
        color: inks[i % inks.length],
      }));
  }

  private colorOf(n: GraphNode): string {
    const cs = getComputedStyle(document.documentElement);
    if (n.kind === "attachment") return cs.getPropertyValue("--color-neutral-500").trim();
    if (n.kind === "tag") {
      const g = this.prefs.colorBy === "tag" && this.groups.find((x) => x.key === n.id);
      return g ? g.color : cs.getPropertyValue("--color-accent-2").trim();
    }
    const key = this.groupKey(n, this.tagFreq());
    const g = key ? this.groups.find((x) => x.key === key) : undefined;
    return g ? g.color : cs.getPropertyValue("--color-text").trim();
  }

  private recolor() {
    this.computeGroups();
    for (const n of this.view.nodes) n.color = this.colorOf(n);
    this.renderLegend();
  }

  // ------------------------------------------------------------- chrome

  private renderLegend() {
    const shown = this.groups.slice(0, 6);
    this.legend.hidden = shown.length === 0;
    const active = this.view.groupFilter;
    this.legend.replaceChildren(
      ...shown.map((g) =>
        h(
          "button.g-leg",
          {
            "aria-pressed": String(active === g.color),
            title: active === g.color ? "Show everything" : `Highlight ${g.label}`,
            onclick: () => {
              this.view.setGroup(active === g.color ? null : g.color);
              this.renderLegend();
            },
            style: `--c:${g.color}`,
          },
          h("span.dot"),
          g.label,
          h("span.n", null, String(g.count)),
        ),
      ),
      this.groups.length > 6 ? h("span.g-more", null, `+${this.groups.length - 6}`) : "",
    );
  }

  private chip(label: string, on: boolean, toggle: () => void, title: string, ic?: string) {
    return h("button.g-chip", { "aria-pressed": String(on), onclick: toggle, title }, ic ? icon(ic) : h("span.dot"), label);
  }

  private renderBars() {
    const p = this.prefs;
    const depth = h(
      "select.g-select",
      {
        title: "Show the whole vault, or only notes this many links from the open note",
        onchange: (e: Event) => this.set({ depth: Number((e.target as HTMLSelectElement).value) }),
      },
      ...[
        [0, "Whole vault"],
        [1, "1 step from note"],
        [2, "2 steps from note"],
        [3, "3 steps from note"],
      ].map(([v, l]) => h("option", { value: String(v), selected: p.depth === v }, String(l))),
    );
    const colorBy = h(
      "div.g-seg",
      { role: "group", "aria-label": "Colour by" },
      h("button", { "aria-pressed": String(p.colorBy === "folder"), onclick: () => this.set({ colorBy: "folder" }) }, "Folders"),
      h("button", { "aria-pressed": String(p.colorBy === "tag"), onclick: () => this.set({ colorBy: "tag" }) }, "Tags"),
    );
    const searchPill = h(
      "div.g-pill",
      null,
      h("label.g-search", null, icon("magnifying-glass"), this.filter, h("kbd", null, "/")),
      h("span.g-sep"),
      h("span.g-label", null, "Colour"),
      colorBy,
      h("span.g-sep"),
      h("span.g-selwrap", null, depth, icon("caret-down")),
    );
    const togglePill = h(
      "div.g-pill",
      null,
      this.chip("Tags", p.showTags, () => this.set({ showTags: !p.showTags }), "Show tags as dots"),
      this.chip("Missing", p.showUnresolved, () => this.set({ showUnresolved: !p.showUnresolved }), "Show links to notes not written yet"),
      this.chip("Files", p.showAttachments, () => this.set({ showAttachments: !p.showAttachments }), "Show images and other attachments"),
      this.chip("Orphans", p.orphans, () => this.set({ orphans: !p.orphans }), "Show notes with no links"),
      this.chip("Arrows", p.arrows, () => this.set({ arrows: !p.arrows }, false), "Show which way links point", "arrow-right"),
      this.chip("Motion", p.motion, () => this.set({ motion: !p.motion }, false), "Drift and travelling signals", "sparkle"),
      h("span.g-sep"),
      h(
        "button.g-chip.g-forces",
        { "aria-pressed": String(!this.drawer.hidden), onclick: () => this.toggleDrawer() },
        icon("sliders-horizontal"),
        "Forces",
        icon(this.drawer.hidden ? "caret-down" : "caret-up"),
      ),
    );
    const keepFocus = document.activeElement === this.filter;
    this.bars.replaceChildren(searchPill, togglePill);
    if (keepFocus) this.filter.focus();
  }

  private toggleDrawer() {
    this.drawer.hidden = !this.drawer.hidden;
    this.renderDrawer();
    this.renderBars();
  }

  private renderDrawer() {
    if (this.drawer.hidden) return;
    const f = this.prefs.forces;
    const slider = (label: string, key: keyof Forces, min: number, max: number, stepv: number, fmt: (v: number) => string) => {
      const out = h("span.v", null, fmt(f[key]));
      const input = h("input", { type: "range", min: String(min), max: String(max), step: String(stepv), value: String(f[key]) }) as HTMLInputElement;
      input.addEventListener("input", () => {
        const v = Number(input.value);
        this.prefs.forces = { ...this.prefs.forces, [key]: v };
        out.textContent = fmt(v);
        this.view.setForces(this.prefs.forces);
        this.save();
      });
      return h("div.g-slider", null, h("div.row", null, h("span", null, label), out), input);
    };
    const notes = this.data.nodes.length;
    const edges = this.data.edges.length;
    this.drawer.replaceChildren(
      h(
        "div.g-card-head",
        null,
        h("span", null, icon("graph"), "Forces"),
        h("button.g-x", { title: "Close", onclick: () => this.toggleDrawer() }, icon("x")),
      ),
      slider("Centre pull", "gravity", 0, 1, 0.01, (v) => v.toFixed(2)),
      slider("Repulsion", "repulsion", 60, 1200, 10, (v) => String(Math.round(v))),
      slider("Link length", "distance", 30, 300, 5, (v) => `${Math.round(v)} px`),
      h(
        "div.g-drawer-foot",
        null,
        h("span", null, "Dots ", h("b", null, String(notes))),
        h("span", null, "Links ", h("b", null, String(edges))),
        h(
          "button.g-link",
          {
            onclick: () => {
              this.prefs.forces = { ...DEFAULT_FORCES };
              this.view.setForces(this.prefs.forces);
              this.save();
              this.renderDrawer();
            },
          },
          "Reset",
        ),
      ),
    );
  }

  // ------------------------------------------------------------- the card

  private async renderCard(n: Node | null) {
    const mine = ++this.cardSeq;
    if (!n) {
      this.card.hidden = true;
      return;
    }
    const { incoming, outgoing } = this.view.degreeOf(n.id);
    const kindLabel =
      n.kind === "tag" ? "Tag" : n.kind === "unresolved" ? "Not written yet" : n.kind === "attachment" ? "Attachment" : parentOf(n.id) || "Vault";
    const snippet = h("p.g-snip", null, n.kind === "note" ? "…" : "");
    const neighbours = this.view
      .neighboursOf(n.id)
      .sort((a, b) => b.degree - a.degree)
      .slice(0, 8);
    this.card.style.setProperty("--c", n.color);
    this.card.replaceChildren(
      h(
        "div.g-card-head",
        null,
        h("div", null, h("div.g-kicker", null, h("span.dot"), kindLabel), h("h3", null, n.kind === "note" ? displayName(n.id) : n.name)),
        h("button.g-x", { title: "Close (Esc)", onclick: () => this.view.select(null) }, icon("x")),
      ),
      n.tags.length ? h("div.g-tags", null, ...n.tags.slice(0, 6).map((t) => h("span.tag", null, t))) : "",
      n.kind === "note" ? snippet : "",
      h(
        "div.g-bento",
        null,
        h("div", null, h("div.k", null, "Incoming", icon("arrow-down-left")), h("div.v", null, String(incoming)), h("div.s", null, incoming === 1 ? "backlink" : "backlinks")),
        h("div", null, h("div.k", null, "Outgoing", icon("arrow-up-right")), h("div.v.acc", null, String(outgoing)), h("div.s", null, outgoing === 1 ? "link" : "links")),
      ),
      neighbours.length
        ? h(
            "div.g-peers",
            null,
            h("div.g-label", null, "Linked with"),
            h(
              "div.g-peerlist",
              null,
              ...neighbours.map((m) =>
                h(
                  "button",
                  { onclick: () => this.view.select(m.id, true), style: `--c:${m.color}` },
                  h("span.dot"),
                  h("span.t", null, m.kind === "note" ? displayName(m.id) : m.name),
                  h("span.d", null, String(m.degree)),
                ),
              ),
            ),
          )
        : "",
      h(
        "div.g-actions",
        null,
        h(
          "button.btn.btn-primary",
          { onclick: () => this.app.open(n, false) },
          n.kind === "tag" ? "Show notes" : n.kind === "unresolved" ? "Create note" : "Open note",
          h("kbd", null, "↵"),
        ),
        n.kind === "note" ? h("button.btn.btn-secondary", { onclick: () => this.app.open(n, true), title: `Open in split pane (${kbd("click")})` }, icon("columns")) : null,
      ),
    );
    this.card.hidden = false;
    if (n.kind === "note") {
      const text = await api.readNote(n.id).catch(() => "");
      if (mine !== this.cardSeq) return;
      snippet.textContent = trunc(plainText(text), 220) || "Empty note.";
    }
  }

  // ------------------------------------------------------------- minimap

  private scheduleMini() {
    if (this.miniTimer) return;
    this.miniTimer = requestAnimationFrame(() => {
      this.miniTimer = 0;
      this.drawMini();
    });
  }

  private miniMap: { s: number; ox: number; oy: number } | null = null;

  private drawMini() {
    if (this.host.offsetParent === null) return;
    const v = this.view;
    this.zoomLabel.textContent = `${Math.round(v.k * 100)}%`;
    const c = this.mini;
    const ctx = c.getContext("2d")!;
    const W = c.width;
    const H = c.height;
    ctx.clearRect(0, 0, W, H);
    const b = v.bounds();
    if (!b) return;
    const pad = 40;
    const bw = b.x1 - b.x0 + pad * 2;
    const bh = b.y1 - b.y0 + pad * 2;
    const s = Math.min(W / bw, H / bh);
    const ox = (W - bw * s) / 2 - (b.x0 - pad) * s;
    const oy = (H - bh * s) / 2 - (b.y0 - pad) * s;
    this.miniMap = { s, ox, oy };
    const cs = getComputedStyle(document.documentElement);
    const ink = cs.getPropertyValue("--color-text").trim();
    ctx.globalAlpha = 0.18;
    ctx.strokeStyle = ink;
    ctx.lineWidth = 1;
    ctx.beginPath();
    for (const l of v.links) {
      const a = l.source as Node;
      const d = l.target as Node;
      if (a.x === undefined || d.x === undefined) continue;
      ctx.moveTo(ox + a.x * s, oy + a.y! * s);
      ctx.lineTo(ox + d.x * s, oy + d.y! * s);
    }
    ctx.stroke();
    ctx.globalAlpha = 1;
    for (const n of v.nodes) {
      if (n.x === undefined) continue;
      ctx.fillStyle = n.color;
      ctx.beginPath();
      ctx.arc(ox + n.x * s, oy + n.y! * s, Math.max(2, n.r * s * 1.4), 0, Math.PI * 2);
      ctx.fill();
    }
    // The part of the graph on screen.
    const { w, h: hh } = v.size;
    const rx = ox + (-v.tx / v.k) * s;
    const ry = oy + (-v.ty / v.k) * s;
    const rw = (w / v.k) * s;
    const rh = (hh / v.k) * s;
    const accent = cs.getPropertyValue("--color-accent").trim();
    ctx.fillStyle = accent;
    ctx.globalAlpha = 0.08;
    ctx.fillRect(rx, ry, rw, rh);
    ctx.globalAlpha = 0.9;
    ctx.strokeStyle = accent;
    ctx.lineWidth = 3;
    ctx.strokeRect(rx, ry, rw, rh);
    ctx.globalAlpha = 1;
  }

  private bindMinimap() {
    const c = this.mini;
    const go = (e: PointerEvent) => {
      const m = this.miniMap;
      if (!m) return;
      const r = c.getBoundingClientRect();
      const px = ((e.clientX - r.left) / r.width) * c.width;
      const py = ((e.clientY - r.top) / r.height) * c.height;
      this.view.panTo((px - m.ox) / m.s, (py - m.oy) / m.s);
    };
    c.addEventListener("pointerdown", (e) => {
      c.setPointerCapture(e.pointerId);
      go(e);
      const move = (ev: PointerEvent) => go(ev);
      c.addEventListener("pointermove", move);
      c.addEventListener("pointerup", () => c.removeEventListener("pointermove", move), { once: true });
    });
  }
}
