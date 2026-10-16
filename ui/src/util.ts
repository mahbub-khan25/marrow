// Small DOM and text helpers shared by the views.

export const $ = <T extends HTMLElement = HTMLElement>(id: string) => document.getElementById(id) as T;

type Child = Node | string | null | undefined | false;

/** `h("button.tab", { title: "x", onclick }, "Label")` */
export function h<K extends keyof HTMLElementTagNameMap>(
  spec: K | `${K}.${string}`,
  props: Record<string, unknown> | null = null,
  ...children: Child[]
): HTMLElementTagNameMap[K] {
  const [tag, ...classes] = spec.split(".");
  const el = document.createElement(tag as K);
  if (classes.length) el.className = classes.join(" ");
  if (props) {
    for (const [k, v] of Object.entries(props)) {
      if (v === undefined || v === null || v === false) continue;
      if (k.startsWith("on") && typeof v === "function") {
        el.addEventListener(k.slice(2), v as EventListener);
      } else if (k === "style" && typeof v === "string") {
        el.style.cssText = v;
      } else if (k === "dataset" && typeof v === "object") {
        Object.assign(el.dataset, v);
      } else if (k in el && typeof v !== "string") {
        (el as unknown as Record<string, unknown>)[k] = v;
      } else {
        el.setAttribute(k, v === true ? "" : String(v));
      }
    }
  }
  for (const c of children) if (c !== null && c !== undefined && c !== false) el.append(c);
  return el;
}

/** A Phosphor duotone icon. */
export const icon = (name: string, style?: string) => h("i", { class: `ph-duotone ph-${name}`, style });

export const isMac = /Mac|iPhone|iPad/.test(navigator.platform);

/** "Ctrl+K" on Windows/Linux, "⌘K" on a Mac. */
export function kbd(key: string, shift = false): string {
  if (isMac) return `${shift ? "⇧" : ""}⌘${key}`;
  return `Ctrl+${shift ? "Shift+" : ""}${key}`;
}

export function ago(ms: number): string {
  const m = Math.round((Date.now() - ms) / 60000);
  if (m < 1) return "just now";
  if (m < 60) return `${m} min ago`;
  const hrs = Math.round(m / 60);
  if (hrs < 24) return `${hrs} h ago`;
  const d = Math.round(hrs / 24);
  if (d === 1) return "yesterday";
  if (d < 30) return `${d} days ago`;
  return new Date(ms).toLocaleDateString(undefined, { day: "numeric", month: "short", year: "numeric" });
}

/** Markdown without its markup, for word counts and excerpts. */
export function plainText(md: string): string {
  return stripFrontmatter(md)
    .text.replace(/```[\s\S]*?```/g, " ")
    .replace(/!?\[\[([^\]|]+)(?:\|([^\]]+))?\]\]/g, (_m, a: string, b?: string) => b ?? a.split("#")[0])
    .replace(/!?\[([^\]]*)\]\([^)]*\)/g, "$1")
    .replace(/(^|\s)#[\p{L}\p{N}_\-/]+/gu, "$1")
    .replace(/^\s*[-*+] \[[ xX]\] /gm, "")
    .replace(/^\s*>\s*\[![\w-]+\][+-]?[ \t]*/gm, "")
    .replace(/^\s*(#{1,6}|>|[-*+]|\d+\.)\s+/gm, "")
    .replace(/[*_`~=]/g, "")
    .replace(/\s+/g, " ")
    .trim();
}

export function wordCount(md: string): number {
  const t = plainText(md);
  return t ? t.split(" ").length : 0;
}

export function trunc(s: string, n: number): string {
  return s.length > n ? s.slice(0, n - 1).trimEnd() + "…" : s;
}

/** Splits off a leading `---` YAML block; `lines` is how many lines it took. */
export function stripFrontmatter(md: string): { text: string; lines: number; yaml: string } {
  const m = /^---\r?\n([\s\S]*?)\r?\n(?:---|\.\.\.)[ \t]*(?:\r?\n|$)/.exec(md);
  if (!m) return { text: md, lines: 0, yaml: "" };
  const lines = m[0].split("\n").length - (m[0].endsWith("\n") ? 1 : 0);
  return { text: md.slice(m[0].length), lines, yaml: m[1] };
}

export interface Heading {
  level: number;
  text: string;
  /** 0-based line in the full note. */
  line: number;
}

export function headings(md: string): Heading[] {
  const out: Heading[] = [];
  let fence = false;
  md.split("\n").forEach((ln, i) => {
    if (/^\s*(```|~~~)/.test(ln)) fence = !fence;
    if (fence) return;
    const m = /^(#{1,6})\s+(.+?)\s*#*\s*$/.exec(ln);
    if (m) {
      const text = m[2].replace(/\[\[([^\]|]+)(?:\|([^\]]+))?\]\]/g, (_x, a: string, b?: string) => b ?? a).replace(/[*_`]/g, "");
      out.push({ level: m[1].length, text, line: i });
    }
  });
  // Frontmatter isn't a heading.
  const fm = stripFrontmatter(md).lines;
  return out.filter((x) => x.line >= fm);
}

/** Local date as YYYY-MM-DD. */
export function isoDay(d = new Date()): string {
  const p = (n: number) => String(n).padStart(2, "0");
  return `${d.getFullYear()}-${p(d.getMonth() + 1)}-${p(d.getDate())}`;
}

export const isDayName = (s: string) => /^\d{4}-\d{2}-\d{2}$/.test(s);

export function longDate(day: string): string {
  const [y, m, d] = day.split("-").map(Number);
  return new Date(y, m - 1, d).toLocaleDateString(undefined, {
    weekday: "long",
    day: "numeric",
    month: "long",
    year: "numeric",
  });
}

export function storage<T>(key: string, fallback: T): T {
  try {
    const raw = localStorage.getItem(key);
    return raw === null ? fallback : (JSON.parse(raw) as T);
  } catch {
    return fallback;
  }
}

export function store(key: string, value: unknown) {
  try {
    localStorage.setItem(key, JSON.stringify(value));
  } catch {
    /* storage unavailable: not critical */
  }
}
