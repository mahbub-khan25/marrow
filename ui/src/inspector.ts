// The inspector beside a note: its local graph, outline, linked mentions
// and outgoing links.

import { api, displayName, type BacklinkGroup, type Mention } from "./api";
import { props } from "./frontmatter";
import { linkTarget } from "./preview";
import { $, h, headings, storage, store, trunc } from "./util";

export interface InspectorApp {
  openPath(path: string, at?: { line: number; ch: number; len?: number }, inSplit?: boolean): void;
  /** Creates (or opens) a link target that doesn't exist yet. */
  openLink(target: string, inSplit?: boolean): void;
  jumpToLine(line: number): void;
  /** Turns a plain-text mention into a link to `target`. */
  linkMention(m: Mention, target: string): Promise<void>;
}

const SVG = "http://www.w3.org/2000/svg";
const svg = (tag: string, attrs: Record<string, string | number>) => {
  const el = document.createElementNS(SVG, tag);
  for (const [k, v] of Object.entries(attrs)) el.setAttribute(k, String(v));
  return el;
};

export class Inspector {
  private path: string | null = null;
  private seq = 0;

  constructor(private app: InspectorApp) {}

  clear() {
    this.path = null;
    for (const id of ["local-graph", "outline", "mentions", "outgoing", "unlinked"]) $(id).replaceChildren();
    $("mention-count").textContent = "";
    $("unlinked-count").textContent = "";
  }

  /** Re-reads links for `path`. `text` is the note as it is in the editor. */
  async show(path: string, text: string) {
    if (path !== this.path) this.showAllUnlinked = false;
    this.path = path;
    this.outline(text);
    const mine = ++this.seq;
    let [groups, links] = await Promise.all([
      api.backlinks(path).catch(() => [] as BacklinkGroup[]),
      api.noteLinks(path).catch(() => ({ outgoing: [] as string[], unresolved: [] as string[] })),
    ]);
    if (mine !== this.seq || this.path !== path) return;
    // A link to a heading in the same note isn't a mention.
    groups = groups.filter((g) => g.source !== path);
    links.outgoing = links.outgoing.filter((p) => p !== path);
    this.mentions(groups);
    this.outgoing(text, links.outgoing, links.unresolved);
    void this.unlinked(path, text);
    void this.localGraph(path, links.outgoing, groups.map((g) => g.source));
  }

  /** Cheap update while typing: only the outline depends on the text alone. */
  textChanged(text: string) {
    this.outline(text);
  }

  private outline(text: string) {
    const hs = headings(text);
    const box = $("outline");
    if (hs.length === 0) {
      box.replaceChildren(h("div.insp-empty", null, "No headings yet."));
      return;
    }
    const min = Math.min(...hs.map((x) => x.level));
    box.replaceChildren(
      ...hs.map((x) =>
        h(
          "button",
          { style: `--indent:${8 + (x.level - min) * 16}px`, onclick: () => this.app.jumpToLine(x.line) },
          x.text,
        ),
      ),
    );
  }

  private mentions(groups: BacklinkGroup[]) {
    const total = groups.length;
    $("mention-count").textContent = total ? String(total) : "";
    const box = $("mentions");
    if (total === 0) {
      box.replaceChildren(h("div.insp-empty", null, "Nothing links here yet."));
      return;
    }
    box.replaceChildren(
      ...groups.map((g) => {
        const first = g.items[0];
        const b = h(
          "button.mention",
          {
            title: g.source,
            onclick: (e: MouseEvent) => this.app.openPath(g.source, first, e.ctrlKey || e.metaKey),
          },
          h("span.t", null, g.title),
        );
        for (const it of g.items.slice(0, 2)) b.append(h("span.s", null, trunc(cleanSnippet(it.snippet), 140)));
        return b;
      }),
    );
  }

  /** Notes that name this one (or an alias) without linking to it. */
  private showAllUnlinked = false;
  private async unlinked(path: string, text: string) {
    const names = [displayName(path)];
    const al = props(text).aliases;
    for (const a of Array.isArray(al) ? al : typeof al === "string" ? [al] : []) if (String(a).trim()) names.push(String(a));
    const mine = this.seq;
    const found = await api.unlinkedMentions(path, names).catch(() => [] as Mention[]);
    if (mine !== this.seq || this.path !== path) return;
    const box = $("unlinked");
    $("unlinked-count").textContent = found.length ? String(found.length) : "";
    if (!found.length) {
      box.replaceChildren(h("div.insp-empty", null, "No other note names this one without linking it."));
      return;
    }
    const byNote = new Map<string, Mention[]>();
    for (const m of found) (byNote.get(m.path) ?? byNote.set(m.path, []).get(m.path)!).push(m);
    const groups = [...byNote.values()];
    const shown = this.showAllUnlinked ? groups : groups.slice(0, 5);
    const items: HTMLElement[] = shown.map((ms) => {
      const g = h("div.unlinked");
      g.append(h("div.t", null, h("button.unlinked-open", { title: ms[0].path, onclick: (e: MouseEvent) => this.app.openPath(ms[0].path, { line: ms[0].line, ch: ms[0].ch, len: ms[0].len }, e.ctrlKey || e.metaKey) }, ms[0].name)));
      for (const m of ms.slice(0, 3)) {
        const before = m.text.slice(0, m.ch);
        const after = m.text.slice(m.ch + m.len);
        const start = Math.max(0, before.length - 60);
        const snip = h(
          "button.s",
          { onclick: (e: MouseEvent) => this.app.openPath(m.path, { line: m.line, ch: m.ch, len: m.len }, e.ctrlKey || e.metaKey) },
          (start > 0 ? "…" : "") + before.slice(start).replace(/^\s*(?:[-*+]|\d+\.)\s+/, ""),
          h("mark", null, m.text.slice(m.ch, m.ch + m.len)),
          trunc(after, 70),
        );
        const link = h(
          "button.link-it",
          {
            title: `Turn “${m.matched}” into a link to this note`,
            onclick: async (e: Event) => {
              (e.currentTarget as HTMLButtonElement).disabled = true;
              await this.app.linkMention(m, path);
            },
          },
          "Link",
        );
        g.append(h("div.unlinked-row", null, snip, link));
      }
      if (ms.length > 3) g.append(h("div.more", null, `and ${ms.length - 3} more in this note`));
      return g;
    });
    if (groups.length > 5 && !this.showAllUnlinked) {
      items.push(
        h(
          "button.show-all",
          {
            onclick: () => {
              this.showAllUnlinked = true;
              void this.unlinked(path, text);
            },
          },
          `Show all ${groups.length} notes`,
        ),
      );
    }
    box.replaceChildren(...items);
  }

  private outgoing(text: string, resolved: string[], unresolved: string[]) {
    const box = $("outgoing");
    // In the order they appear, each once.
    const seen = new Set<string>();
    const items: HTMLElement[] = [];
    const missing = new Set(unresolved.map((u) => u.replace(/\.md$/i, "").toLowerCase()));
    const resolvedNames = new Map(resolved.map((p) => [displayName(p).toLowerCase(), p]));
    for (const m of text.matchAll(/!?\[\[([^\[\]\n]+?)\]\]/g)) {
      const target = linkTarget(m[1]);
      if (!target) continue;
      const key = target.replace(/\.md$/i, "").toLowerCase();
      if (seen.has(key)) continue;
      seen.add(key);
      if (missing.has(key)) {
        items.push(
          h(
            "button.missing",
            { title: `Create “${target}”`, onclick: (e: MouseEvent) => this.app.openLink(target, e.ctrlKey || e.metaKey) },
            h("span.t", null, target),
            h("span.w", null, "not written yet"),
          ),
        );
      } else {
        const base = key.split("/").pop()!;
        const path = resolvedNames.get(base) ?? resolved.find((p) => p.toLowerCase().replace(/\.md$/, "").endsWith(key));
        items.push(
          h(
            "button",
            {
              title: path ?? target,
              onclick: (e: MouseEvent) => (path ? this.app.openPath(path, undefined, e.ctrlKey || e.metaKey) : this.app.openLink(target, e.ctrlKey || e.metaKey)),
            },
            h("span.t", null, path ? displayName(path) : target),
          ),
        );
      }
    }
    // Markdown links and anything else the engine resolved but the regex missed.
    for (const p of resolved) {
      const key = displayName(p).toLowerCase();
      if ([...seen].some((s) => s === key || s.endsWith("/" + key))) continue;
      seen.add(key);
      items.push(h("button", { title: p, onclick: () => this.app.openPath(p) }, h("span.t", null, displayName(p))));
    }
    box.replaceChildren(...(items.length ? items : [h("div.insp-empty", null, "No links yet. Type [[ to add one.")]));
  }

  /** Links one step away, or two (neighbours of neighbours on an outer ring). */
  private depth: 1 | 2 = storage<1 | 2>("marrow.localDepth", 1);
  private last: { path: string; outgoing: string[]; incoming: string[] } | null = null;

  private depthSeg(): HTMLElement {
    const seg = h("div.seg.tiny", { role: "group", "aria-label": "Steps shown" });
    for (const d of [1, 2] as const) {
      seg.append(
        h(
          "button",
          {
            "aria-pressed": String(this.depth === d),
            title: d === 1 ? "Notes linked to or from this one" : "Also the notes those link with",
            onclick: () => {
              this.depth = d;
              store("marrow.localDepth", d);
              if (this.last) void this.localGraph(this.last.path, this.last.outgoing, this.last.incoming);
            },
          },
          d === 1 ? "1 step" : "2 steps",
        ),
      );
    }
    return seg;
  }

  /** The note in the middle, its neighbours around it. Solid lines go out, dashed come in. */
  private async localGraph(path: string, outgoing: string[], incoming: string[]) {
    this.last = { path, outgoing, incoming };
    const head = $("local-graph").previousElementSibling as HTMLElement;
    head.querySelector(".seg")?.remove();
    head.append(this.depthSeg());

    const out = new Set(outgoing.filter((p) => p !== path && p.toLowerCase().endsWith(".md")));
    const ring1 = [...new Set([...out, ...incoming.filter((p) => p !== path)])].slice(0, 10);
    // Second ring: each outer note hangs off the first-ring note it links with.
    const ring2: { path: string; parent: number }[] = [];
    if (this.depth === 2 && ring1.length) {
      const g = await api.graph({ showUnresolved: false, showAttachments: false, showTags: false }).catch(() => null);
      if (this.last?.path !== path) return;
      if (g) {
        const adj = new Map<string, Set<string>>();
        for (const e of g.edges) {
          (adj.get(e.source) ?? adj.set(e.source, new Set()).get(e.source)!).add(e.target);
          (adj.get(e.target) ?? adj.set(e.target, new Set()).get(e.target)!).add(e.source);
        }
        // Take one from each first-ring note in turn, so the ring stays balanced.
        const seen = new Set([path, ...ring1]);
        const queues = ring1.map((p) => [...(adj.get(p) ?? [])].sort());
        for (let added = true; added && ring2.length < 18; ) {
          added = false;
          queues.forEach((q, i) => {
            while (q.length && ring2.length < 18) {
              const next = q.shift()!;
              if (seen.has(next)) continue;
              seen.add(next);
              ring2.push({ path: next, parent: i });
              added = true;
              break;
            }
          });
        }
      }
    }

    const box = $("local-graph");
    const W = 280,
      H = 200,
      cx = 140,
      cy = 96;
    const two = ring2.length > 0;
    const r1 = two ? { x: 62, y: 42 } : { x: 100, y: 66 };
    const s = svg("svg", { viewBox: `0 0 ${W} ${H}` });
    const n = ring1.length;
    const angle1 = (i: number) => -Math.PI / 2 + (i / Math.max(n, 1)) * Math.PI * 2;
    const pos = ring1.map((_, i) => ({ x: cx + Math.cos(angle1(i)) * r1.x, y: cy + Math.sin(angle1(i)) * r1.y }));
    // Outer ring: spread each parent's children around the parent's angle.
    const byParent = new Map<number, number[]>();
    ring2.forEach((r, k) => (byParent.get(r.parent) ?? byParent.set(r.parent, []).get(r.parent)!).push(k));
    const pos2 = ring2.map((r, k) => {
      const sibs = byParent.get(r.parent)!;
      const j = sibs.indexOf(k);
      const spread = Math.min(0.9, (Math.PI * 2) / Math.max(n, 1) * 0.8);
      const a = angle1(r.parent) + (sibs.length > 1 ? (j / (sibs.length - 1) - 0.5) * spread : 0);
      return { x: cx + Math.cos(a) * 128, y: cy + Math.sin(a) * 88 };
    });
    ring2.forEach((r, k) => {
      s.append(svg("line", { x1: pos[r.parent].x, y1: pos[r.parent].y, x2: pos2[k].x, y2: pos2[k].y, class: "far" }));
    });
    ring1.forEach((p, i) => {
      s.append(svg("line", { x1: cx, y1: cy, x2: pos[i].x, y2: pos[i].y, "stroke-dasharray": out.has(p) ? "" : "3 3" }));
    });
    const dot = (p: string, x: number, y: number, r: number, cls: string) => {
      const c = svg("circle", { cx: x, cy: y, r, class: cls });
      const t = document.createElementNS(SVG, "title");
      t.textContent = displayName(p);
      c.append(t);
      c.addEventListener("click", (e) => this.app.openPath(p, undefined, (e as MouseEvent).ctrlKey));
      s.append(c);
    };
    ring2.forEach((r, k) => dot(r.path, pos2[k].x, pos2[k].y, 3.5, "n far"));
    ring1.forEach((p, i) => dot(p, pos[i].x, pos[i].y, two ? 5 : 6, "n"));
    s.append(svg("circle", { cx, cy, r: 9, class: "me" }));
    const labels = ring1.map((p, i) => {
      const { x, y } = pos[i];
      const below = y > cy;
      return h(
        "button.lbl",
        {
          style: `left:${(x / W) * 100}%;top:${((below ? y + 8 : y - 24) / H) * 100}%`,
          onclick: (e: MouseEvent) => this.app.openPath(p, undefined, e.ctrlKey || e.metaKey),
          title: p,
        },
        trunc(displayName(p), two || n > 6 ? 13 : 18),
      );
    });
    box.replaceChildren(s, ...labels);
    if (n === 0) box.append(h("div.insp-empty", { style: "position:absolute;left:0;right:0;bottom:4px;text-align:center" }, "No links yet."));
  }
}

/** Backlink snippets come as raw Markdown lines; tidy them for reading. */
function cleanSnippet(s: string): string {
  return s
    .replace(/^\s*(?:[-*+]|\d+\.)\s+(\[[ xX]\]\s+)?/, "")
    .replace(/^#+\s+/, "")
    .replace(/!?\[\[([^\]|]+)(?:\|([^\]]+))?\]\]/g, (_m, a: string, b?: string) => b ?? a)
    .replace(/[*_`]/g, "")
    .trim();
}

