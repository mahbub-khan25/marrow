// Database views: a ```database block shows the notes in a folder as a
// table, a board, a calendar or a gallery, using their properties. Editing a
// cell, dragging a card or adding a row writes to those notes' frontmatter,
// so everything stays plain Markdown. Obsidian shows the block as code.
//
//   ```database
//   from: Projects          # folder (default: this note's folder)
//   view: board             # table | board | calendar | gallery
//   columns: [status, due, project, project.owner, count(backlinks)]
//   group: status           # board columns
//   date: due               # calendar
//   sort: due desc
//   filter: status != done and due < today
//   ```
//
// Columns can follow relations (`project.status` reads status from the notes
// linked in `project`) and roll values up: count(), sum(), avg(), min(),
// max(), percent() — e.g. `Done = percent(backlinks.done)`.

import { isMap, parseDocument } from "yaml";

import { api, displayName, parentOf, type NoteRow, type QueryResult } from "./api";
import { findDoc, replaceText, save } from "./docs";
import { setProp } from "./frontmatter";
import { openAiColumn } from "./ai-column";
import { h, icon, isoDay } from "./util";

export type View = "table" | "board" | "calendar" | "gallery";

export interface DbConfig {
  from: string;
  subfolders: boolean;
  view: View;
  columns: string[];
  group: string;
  date: string;
  sort: string;
  filter: string;
  title: string;
  /** AI columns: property → the instruction that fills it. */
  ai: Record<string, string>;
}

export interface DbHost {
  /** The note the block is in. */
  source: string;
  openPath(path: string, inSplit: boolean): void;
  /** Rewrites the block's YAML (switching views, sorting). */
  setConfig(key: keyof DbConfig, value: unknown): void;
  assetUrl(path: string): string;
}

const VIEWS: [View, string, string][] = [
  ["table", "Table", "table"],
  ["board", "Board", "kanban"],
  ["calendar", "Calendar", "calendar-dots"],
  ["gallery", "Gallery", "squares-four"],
];

// ------------------------------------------------------------------ config

export function parseConfig(body: string, source: string): DbConfig {
  let raw: Record<string, unknown> = {};
  try {
    const v = parseDocument(body).toJS() as unknown;
    if (v && typeof v === "object" && !Array.isArray(v)) raw = v as Record<string, unknown>;
  } catch {
    /* an empty or broken block shows the defaults */
  }
  const str = (k: string) => (raw[k] === undefined || raw[k] === null ? "" : String(raw[k]).trim());
  const view = str("view").toLowerCase();
  let columns: string[] = [];
  if (Array.isArray(raw.columns)) columns = raw.columns.map(String);
  else if (typeof raw.columns === "string") columns = splitTop(raw.columns);
  const from = str("from").replace(/^\[\[|\]\]$/g, "").replace(/^\/+|\/+$/g, "");
  return {
    from: from || parentOf(source),
    subfolders: raw.subfolders === true,
    view: (VIEWS.some((v) => v[0] === view) ? view : "table") as View,
    columns,
    group: str("group"),
    date: str("date"),
    sort: str("sort"),
    filter: str("filter"),
    title: str("title"),
    ai:
      raw.ai && typeof raw.ai === "object" && !Array.isArray(raw.ai)
        ? Object.fromEntries(Object.entries(raw.ai as Record<string, unknown>).map(([k, v]) => [k, String(v ?? "")]))
        : {},
  };
}

/** Sets one key in the block's YAML, keeping the rest as written. */
export function setConfigKey(body: string, key: string, value: unknown): string {
  const doc = parseDocument(body || "{}");
  if (doc.errors.length || (doc.contents !== null && !isMap(doc.contents))) return body;
  if (value === undefined || value === "") doc.delete(key);
  else if (Array.isArray(value)) {
    // Lists stay on one line, as people write them: [a, b, c]
    const node = doc.createNode(value);
    node.flow = true;
    doc.set(key, node);
  } else doc.set(key, value);
  const out = doc.toString({ flowCollectionPadding: false, lineWidth: 0 }).trimEnd();
  return out === "{}" ? "" : out;
}

/** Splits on commas that aren't inside parentheses. */
function splitTop(s: string): string[] {
  const out: string[] = [];
  let depth = 0;
  let cur = "";
  for (const c of s) {
    if (c === "(") depth++;
    if (c === ")") depth--;
    if (c === "," && depth === 0) {
      out.push(cur.trim());
      cur = "";
    } else cur += c;
  }
  if (cur.trim()) out.push(cur.trim());
  return out;
}

// ------------------------------------------------------------- expressions

const AGG = /^(count|sum|avg|min|max|percent)\((.*)\)$/i;
const SPECIAL = new Set(["title", "name", "file", "created", "modified", "tags", "folder", "backlinks"]);

interface Column {
  label: string;
  expr: string;
  fn: string | null;
  path: string[];
  /** A plain property of the row itself, so it can be edited. */
  editable: boolean;
}

function column(spec: string): Column {
  let label = "";
  let expr = spec.trim();
  const eq = /^([^=()]+?)\s*=\s*(.+)$/.exec(expr);
  if (eq && !/[!<>]$/.test(eq[1])) {
    label = eq[1].trim();
    expr = eq[2].trim();
  }
  const m = AGG.exec(expr);
  const fn = m ? m[1].toLowerCase() : null;
  const path = (m ? m[2] : expr).split(".").map((s) => s.trim()).filter(Boolean);
  const editable = !fn && path.length === 1 && !SPECIAL.has(path[0].toLowerCase());
  if (!label) {
    const p = path.join(" → ");
    label = fn ? `${fn[0].toUpperCase()}${fn.slice(1)} ${p}` : p;
  }
  return { label, expr, fn, path, editable };
}

type Val = unknown;

class Ctx {
  byPath = new Map<string, NoteRow>();
  constructor(q: QueryResult) {
    for (const r of [...q.rows, ...q.related]) this.byPath.set(r.path, r);
  }

  /** The values a path reaches from a row (relations fan out). */
  values(row: NoteRow, path: string[]): Val[] {
    if (!path.length) return [row];
    const [first, ...rest] = path;
    const key = first.toLowerCase();
    const follow = (paths: string[]) =>
      paths
        .map((p) => this.byPath.get(p))
        .filter((r): r is NoteRow => !!r)
        .flatMap((r) => this.values(r, rest));
    if (key === "backlinks") return follow(row.backlinks);
    if (rest.length) return follow(propLinks(row, first));
    if (key === "title" || key === "name" || key === "file") return [row];
    if (key === "created") return [isoDay(new Date(row.createdMs))];
    if (key === "modified") return [isoDay(new Date(row.modifiedMs))];
    if (key === "tags" && !(first in row.props)) return [row.tags.map((t) => t.replace(/^#/, ""))];
    if (key === "folder") return [parentOf(row.path)];
    return [prop(row, first)];
  }

  cell(row: NoteRow, c: Column): Val {
    const vs = this.values(row, c.path);
    if (!c.fn) return vs.length === 1 ? vs[0] : vs;
    const flat = vs.flatMap((v) => (Array.isArray(v) ? v : [v])).filter((v) => v !== null && v !== undefined && v !== "");
    const nums = flat.map(Number).filter((n) => Number.isFinite(n));
    switch (c.fn) {
      case "count":
        return flat.length;
      case "sum":
        return round(nums.reduce((a, b) => a + b, 0));
      case "avg":
        return nums.length ? round(nums.reduce((a, b) => a + b, 0) / nums.length) : null;
      case "min":
        return nums.length ? Math.min(...nums) : flat.length ? [...flat].map(String).sort()[0] : null;
      case "max":
        return nums.length ? Math.max(...nums) : flat.length ? [...flat].map(String).sort().pop() : null;
      case "percent": {
        const all = vs.flatMap((v) => (Array.isArray(v) ? v : [v]));
        if (!all.length) return null;
        const yes = all.filter((v) => v === true || v === "true" || v === "yes" || v === "done" || v === "x").length;
        return `${Math.round((yes / all.length) * 100)}%`;
      }
    }
    return null;
  }
}

const round = (n: number) => Math.round(n * 100) / 100;

/** A property, matched without regard to case. */
function prop(row: NoteRow, key: string): Val {
  if (key in row.props) return row.props[key];
  const k = Object.keys(row.props).find((x) => x.toLowerCase() === key.toLowerCase());
  return k ? row.props[k] : undefined;
}

function propLinks(row: NoteRow, key: string): string[] {
  if (row.propLinks[key]) return row.propLinks[key];
  const k = Object.keys(row.propLinks).find((x) => x.toLowerCase() === key.toLowerCase());
  return k ? row.propLinks[k] : [];
}

// ----------------------------------------------------------------- filters

interface Cond {
  left: string[];
  fn: string | null;
  op: string;
  right: string;
}

function parseFilter(s: string): Cond[] {
  return s
    .split(/\s+and\s+/i)
    .map((part) => /^(.+?)\s*(!=|>=|<=|=|>|<|!contains|contains)\s*(.*)$/i.exec(part.trim()))
    .filter((m): m is RegExpExecArray => !!m)
    .map((m) => {
      const c = column(m[1]);
      let right = m[3].trim().replace(/^["']|["']$/g, "");
      if (right.toLowerCase() === "today") right = isoDay();
      return { left: c.path, fn: c.fn, op: m[2].toLowerCase(), right };
    });
}

function text(v: Val): string {
  if (v === null || v === undefined) return "";
  if (typeof v === "object" && v && "title" in (v as object)) return (v as NoteRow).title;
  if (Array.isArray(v)) return v.map(text).join(", ");
  return String(v).replace(/^\[\[|\]\]$/g, "").split("|")[0];
}

function matches(ctx: Ctx, row: NoteRow, c: Cond): boolean {
  const col: Column = { label: "", expr: "", fn: c.fn, path: c.left, editable: false };
  const v = ctx.cell(row, col);
  const list = (Array.isArray(v) ? v : [v]).map(text).map((s) => s.toLowerCase());
  const r = c.right.toLowerCase();
  const empty = r === "empty" || r === "";
  const num = (s: string) => (s !== "" && Number.isFinite(Number(s)) ? Number(s) : null);
  const cmp = (a: string) => {
    const x = num(a);
    const y = num(r);
    return x !== null && y !== null ? x - y : a.localeCompare(r);
  };
  const any = (f: (s: string) => boolean) => (list.length ? list.some(f) : f(""));
  switch (c.op) {
    case "=":
      return empty ? list.every((s) => s === "") : any((s) => s === r);
    case "!=":
      return empty ? list.some((s) => s !== "") : !any((s) => s === r);
    case "contains":
      return any((s) => s.includes(r));
    case "!contains":
      return !any((s) => s.includes(r));
    case ">":
      return any((s) => s !== "" && cmp(s) > 0);
    case "<":
      return any((s) => s !== "" && cmp(s) < 0);
    case ">=":
      return any((s) => s !== "" && cmp(s) >= 0);
    case "<=":
      return any((s) => s !== "" && cmp(s) <= 0);
  }
  return true;
}

// ------------------------------------------------------------------ writing

/** Edits another note's frontmatter (through its open tab if it has one). */
export async function editNote(path: string, fn: (text: string) => string) {
  const d = findDoc(path);
  if (d) {
    const next = fn(d.text);
    if (next !== d.text) {
      replaceText(d, next);
      // Saved straight away, so views that read the disk (tasks, search) see it.
      await save(d);
    }
    return;
  }
  const text = await api.readNote(path);
  const next = fn(text);
  if (next !== text) await api.writeNote(path, next, text);
}

/** Values implied by "key = value" filters, for new notes. */
function filterDefaults(cfg: DbConfig): Record<string, unknown> {
  const out: Record<string, unknown> = {};
  for (const c of parseFilter(cfg.filter)) {
    if (c.op === "=" && !c.fn && c.left.length === 1 && !SPECIAL.has(c.left[0].toLowerCase()) && c.right && c.right !== "empty") {
      out[c.left[0]] = c.right === "true" ? true : c.right === "false" ? false : c.right;
    }
  }
  return out;
}

async function createRow(cfg: DbConfig, name: string, values: Record<string, unknown>): Promise<string> {
  let text = "";
  for (const [k, v] of Object.entries({ ...filterDefaults(cfg), ...values })) text = setProp(text, k, v);
  return api.createFile(cfg.from, name.trim() || "Untitled", "md", text);
}

// -------------------------------------------------------------- rendering

/** Months shown per calendar block, kept while the app is open. */
const months = new Map<string, string>();

export class Database {
  private cfg: DbConfig;
  private q: QueryResult | null = null;
  private ctx: Ctx | null = null;
  private body = h("div.db-body");
  private timer: number | undefined;
  private readonly onVault = () => {
    if (!this.el.isConnected) {
      window.removeEventListener("marrow-vault-changed", this.onVault);
      return;
    }
    window.clearTimeout(this.timer);
    this.timer = window.setTimeout(() => void this.load(), 250);
  };

  constructor(
    readonly el: HTMLElement,
    configText: string,
    private host: DbHost,
  ) {
    this.cfg = parseConfig(configText, host.source);
    window.addEventListener("marrow-vault-changed", this.onVault);
    void this.load();
  }

  private columns(): Column[] {
    if (this.cfg.columns.length) return this.cfg.columns.map(column);
    // Every property the notes use, in first-seen order.
    const seen: string[] = [];
    for (const r of this.q?.rows ?? []) for (const k of Object.keys(r.props)) if (!seen.includes(k) && !["icon", "cover", "position"].includes(k)) seen.push(k);
    return seen.slice(0, 8).map(column);
  }

  private async load() {
    try {
      this.q = await api.queryNotes(this.cfg.from, this.cfg.subfolders);
    } catch (e) {
      this.el.replaceChildren(h("div.db-error", null, `Can't show this database: ${e}`));
      return;
    }
    this.ctx = new Ctx(this.q);
    this.render();
  }

  /** The rows after filtering and sorting. */
  private rows(): NoteRow[] {
    const ctx = this.ctx!;
    const conds = parseFilter(this.cfg.filter);
    // A database note doesn't list itself.
    let rows = this.q!.rows.filter((r) => r.path !== this.host.source && conds.every((c) => matches(ctx, r, c)));
    const sorts = splitTop(this.cfg.sort).map((s) => {
      const m = /^(.*?)(?:\s+(asc|desc))?$/i.exec(s.trim())!;
      return { col: column(m[1]), dir: m[2]?.toLowerCase() === "desc" ? -1 : 1 };
    });
    if (sorts.length) {
      rows = [...rows].sort((a, b) => {
        for (const s of sorts) {
          const x = text(ctx.cell(a, s.col));
          const y = text(ctx.cell(b, s.col));
          if (x === y) continue;
          // Empty values go last either way.
          if (!x) return 1;
          if (!y) return -1;
          const nx = Number(x);
          const ny = Number(y);
          const d = Number.isFinite(nx) && Number.isFinite(ny) ? nx - ny : x.localeCompare(y, undefined, { numeric: true, sensitivity: "base" });
          if (d) return d * s.dir;
        }
        return 0;
      });
    }
    return rows;
  }

  private render() {
    const cfg = this.cfg;
    const title = cfg.title || (cfg.from ? cfg.from.split("/").pop()! : "All notes");
    const bar = h(
      "div.db-bar",
      null,
      h("div.db-title", null, icon("database"), h("span", null, title)),
      h(
        "div.db-views",
        { role: "tablist" },
        ...VIEWS.map(([v, label, ic]) =>
          h(
            "button",
            {
              role: "tab",
              "aria-selected": String(v === cfg.view),
              "aria-label": label,
              onclick: () => v !== cfg.view && this.host.setConfig("view", v),
            },
            icon(ic),
            h("span", null, label),
          ),
        ),
      ),
    );
    this.body = h(`div.db-body.db-view-${cfg.view}` as "div");
    const rows = this.rows();
    if (cfg.view === "board") this.board(rows);
    else if (cfg.view === "calendar") this.calendar(rows);
    else if (cfg.view === "gallery") this.gallery(rows);
    else this.table(rows);
    this.el.replaceChildren(bar, this.body);
  }

  // ------------------------------------------------------------- shared

  private titleLink(r: NoteRow, cls = "db-link"): HTMLElement {
    const ic = typeof r.props.icon === "string" ? r.props.icon : "";
    return h(
      `span.${cls}` as "span",
      {
        title: r.path,
        onclick: (e: MouseEvent) => this.host.openPath(r.path, e.ctrlKey || e.metaKey),
      },
      ic ? h("span.db-icon", null, ic) : "",
      r.title,
    );
  }

  /** A cell's value for reading. */
  private show(v: Val, c?: Column, row?: NoteRow): Node {
    if (v === null || v === undefined || v === "") return h("span.db-empty");
    // Relations: the linked notes, clickable.
    if (row && c && !c.fn && c.path.length === 1 && propLinks(row, c.path[0]).length) {
      return h(
        "span.db-chips",
        null,
        ...propLinks(row, c.path[0]).map((p) => {
          const r = this.ctx!.byPath.get(p);
          return h("span.db-chip.is-link", { onclick: () => this.host.openPath(p, false) }, r ? r.title : displayName(p));
        }),
      );
    }
    if (typeof v === "object" && !Array.isArray(v) && v && "path" in (v as object)) return this.titleLink(v as NoteRow, "db-chip is-link");
    if (Array.isArray(v)) {
      const items = v.flatMap((x) => (Array.isArray(x) ? x : [x])).filter((x) => x !== null && x !== undefined && x !== "");
      return h("span.db-chips", null, ...items.map((x) => (typeof x === "object" && x && "path" in (x as object) ? this.titleLink(x as NoteRow, "db-chip is-link") : h("span.db-chip", null, text(x)))));
    }
    if (typeof v === "boolean") return h("span.db-check", { "aria-checked": String(v) }, v ? icon("check-square") : icon("square"));
    if (typeof v === "string" && /^\d{4}-\d{2}-\d{2}/.test(v)) return document.createTextNode(prettyDate(v));
    return document.createTextNode(text(v));
  }

  private setValue(row: NoteRow, key: string, value: unknown) {
    void editNote(row.path, (t) => setProp(t, key, value));
  }

  /** Edits a cell in place. */
  private editCell(td: HTMLElement, row: NoteRow, c: Column) {
    const key = Object.keys(row.props).find((k) => k.toLowerCase() === c.path[0].toLowerCase()) ?? c.path[0];
    const v = row.props[key];
    if (typeof v === "boolean") {
      this.setValue(row, key, !v);
      return;
    }
    const isDate = typeof v === "string" && /^\d{4}-\d{2}-\d{2}$/.test(v);
    const isNum = typeof v === "number";
    const isList = Array.isArray(v);
    const input = h("input.db-input", {
      type: isDate ? "date" : isNum ? "number" : "text",
      value: v === null || v === undefined ? "" : isList ? (v as unknown[]).map(String).join(", ") : String(v),
      spellcheck: false,
    }) as HTMLInputElement;
    td.replaceChildren(input);
    input.focus();
    if (!isDate) input.select();
    let done = false;
    const finish = (save: boolean) => {
      if (done) return;
      done = true;
      if (!save) {
        this.render();
        return;
      }
      const raw = input.value;
      const next = isNum ? (raw === "" ? null : Number(raw)) : isList ? raw.split(",").map((s) => s.trim()).filter(Boolean) : raw;
      if (JSON.stringify(next) !== JSON.stringify(v ?? (isList ? [] : ""))) this.setValue(row, key, next);
      else this.render();
    };
    input.addEventListener("keydown", (e) => {
      e.stopPropagation();
      if (e.key === "Enter") finish(true);
      if (e.key === "Escape") finish(false);
    });
    input.addEventListener("blur", () => finish(true));
  }

  /** A one-line box for a new note's name, in place of `anchor`. */
  private askName(anchor: HTMLElement, values: Record<string, unknown>) {
    const input = h("input.db-input.db-new-name", { placeholder: "Name the new note, then press Enter", spellcheck: false }) as HTMLInputElement;
    anchor.replaceWith(input);
    input.focus();
    let done = false;
    const finish = async (save: boolean) => {
      if (done) return;
      done = true;
      if (save && input.value.trim()) {
        try {
          await createRow(this.cfg, input.value, values);
        } catch (e) {
          input.replaceWith(h("div.db-error", null, String(e)));
          return;
        }
      }
      void this.load();
    };
    input.addEventListener("keydown", (e) => {
      e.stopPropagation();
      if (e.key === "Enter") void finish(true);
      if (e.key === "Escape") void finish(false);
    });
    input.addEventListener("blur", () => void finish(true));
  }

  private newButton(label: string, values: Record<string, unknown>, cls = "db-new"): HTMLElement {
    const b: HTMLElement = h(`button.${cls}` as "button", { onclick: () => this.askName(b, values) }, icon("plus"), label);
    return b;
  }

  // -------------------------------------------------------------- table

  private table(rows: NoteRow[]) {
    const cols = this.columns();
    const sortCol = /^(.*?)(?:\s+(asc|desc))?$/i.exec(splitTop(this.cfg.sort)[0] ?? "");
    const sortKey = sortCol?.[1]?.trim() ?? "";
    const sortDir = sortCol?.[2]?.toLowerCase() === "desc" ? "desc" : "asc";
    const head = (label: string, expr: string, ic: string) =>
      h(
        "th",
        {
          title: "Sort by this column",
          onclick: () => this.host.setConfig("sort", sortKey === expr ? (sortDir === "asc" ? `${expr} desc` : "") : expr),
        },
        icon(ic),
        label,
        sortKey === expr ? icon(sortDir === "asc" ? "arrow-up" : "arrow-down") : "",
      );
    const table = h(
      "table.db-table",
      null,
      h(
        "thead",
        null,
        h("tr", null, head("Name", "title", "file-text"), ...cols.map((c) => head(c.label, c.expr, this.cfg.ai[c.expr] ? "sparkle" : c.fn ? "function" : c.path.length > 1 ? "arrow-elbow-down-right" : "text-align-left"))),
      ),
      h(
        "tbody",
        null,
        ...rows.map((r) =>
          h(
            "tr",
            null,
            h("td.db-name", null, this.titleLink(r)),
            ...cols.map((c) => {
              const td = h(`td${c.editable ? ".is-editable" : ""}` as "td", null, this.show(this.ctx!.cell(r, c), c, r));
              if (c.editable) td.addEventListener("click", (e) => !(e.target as HTMLElement).closest(".db-chip") && this.editCell(td, r, c));
              return td;
            }),
          ),
        ),
      ),
    );
    this.body.append(
      h("div.db-scroll", null, table),
      h(
        "div.db-foot",
        null,
        this.newButton("New", {}),
        h("button.db-new", { onclick: () => this.aiColumn(rows) }, icon("sparkle"), "AI column"),
        h("span.db-count", null, `${rows.length} ${rows.length === 1 ? "note" : "notes"}`),
      ),
    );
  }

  /** Adds (or refills) a column the AI writes for each note. */
  private aiColumn(rows: NoteRow[]) {
    openAiColumn({
      rows,
      existing: this.cfg.ai,
      save: (key, instruction) => {
        const cols = this.cfg.columns.length ? this.cfg.columns : this.columns().map((c) => c.expr);
        if (!cols.some((c) => c.toLowerCase() === key.toLowerCase())) this.host.setConfig("columns", [...cols, key]);
        if (this.cfg.ai[key] !== instruction) this.host.setConfig("ai", { ...this.cfg.ai, [key]: instruction });
      },
    });
  }

  // -------------------------------------------------------------- board

  private board(rows: NoteRow[]) {
    const key = this.cfg.group || "status";
    const groupCol = column(key);
    const cols = this.columns().filter((c) => c.expr !== key);
    const groups = new Map<string, NoteRow[]>();
    const label = (r: NoteRow) => text(this.ctx!.cell(r, groupCol));
    // Columns in workflow order (to do → doing → done), others as they appear; empty last.
    const seen: string[] = [];
    for (const r of this.q!.rows) {
      const g = label(r);
      if (g && !seen.includes(g)) seen.push(g);
    }
    seen.sort((a, b) => stage(a) - stage(b));
    for (const g of seen) groups.set(g, []);
    groups.set("", []);
    for (const r of rows) groups.get(label(r))!.push(r);
    const editable = groupCol.editable;
    const board = h("div.db-lanes");
    for (const [g, list] of groups) {
      if (!g && !list.length && groups.size > 1) continue;
      const lane = h(
        "div.db-lane",
        { dataset: { group: g } },
        h("div.db-lane-head", null, h("span.db-pill", null, g || `No ${key}`), h("span.db-count", null, String(list.length))),
        ...list.map((r) => this.card(r, cols, editable)),
        this.newButton("New", g ? { [key]: g } : {}, "db-new.db-lane-new"),
      );
      if (editable) this.dropTarget(lane, (path) => {
        const r = this.ctx!.byPath.get(path);
        if (r && label(r) !== g) this.setValue(r, Object.keys(r.props).find((k) => k.toLowerCase() === key.toLowerCase()) ?? key, g || undefined);
      });
      board.append(lane);
    }
    this.body.append(board);
  }

  private card(r: NoteRow, cols: Column[], draggable: boolean): HTMLElement {
    const fields = cols
      .map((c) => ({ c, v: this.ctx!.cell(r, c) }))
      // Unticked boxes and empty values say nothing on a card.
      .filter(({ v }) => !(v === null || v === undefined || v === "" || v === false || (Array.isArray(v) && !v.length)))
      .slice(0, 5);
    const card = h(
      `div.db-card${draggable ? ".is-draggable" : ""}` as "div",
      { dataset: { path: r.path } },
      this.titleLink(r, "db-card-title"),
      fields.length
        ? h(
            "div.db-fields",
            null,
            ...fields.map(({ c, v }) =>
              h("span.db-field", { title: c.label }, v === true ? h("span.db-chip", null, icon("check"), c.label) : this.show(v, c, r)),
            ),
          )
        : "",
    );
    if (draggable) this.draggable(card, r.path);
    return card;
  }

  private drops = new WeakMap<HTMLElement, (path: string) => void>();

  private dropTarget(el: HTMLElement, onDrop: (path: string) => void) {
    el.dataset.drop = "";
    this.drops.set(el, onDrop);
  }

  /** Lets a card be dragged onto a drop target (a lane or a day). */
  private draggable(el: HTMLElement, path: string) {
    el.addEventListener("pointerdown", (e) => {
      if (e.button !== 0 || (e.target as HTMLElement).closest("input, button")) return;
      e.preventDefault(); // no text selection while dragging; clicks still work
      const x0 = e.clientX;
      const y0 = e.clientY;
      const r = el.getBoundingClientRect();
      let ghost: HTMLElement | null = null;
      let over: HTMLElement | null = null;
      const move = (ev: PointerEvent) => {
        if (!ghost) {
          if (Math.hypot(ev.clientX - x0, ev.clientY - y0) < 5) return;
          ghost = el.cloneNode(true) as HTMLElement;
          ghost.classList.add("db-ghost");
          ghost.style.width = `${r.width}px`;
          document.body.append(ghost);
          el.classList.add("is-dragging");
          document.body.classList.add("block-dragging");
        }
        ghost.style.left = `${ev.clientX - (x0 - r.left)}px`;
        ghost.style.top = `${ev.clientY - (y0 - r.top)}px`;
        const t = document.elementFromPoint(ev.clientX, ev.clientY)?.closest<HTMLElement>("[data-drop]") ?? null;
        const target = t && this.el.contains(t) ? t : null;
        if (target !== over) {
          over?.classList.remove("is-over");
          over = target;
          over?.classList.add("is-over");
        }
      };
      const up = () => {
        window.removeEventListener("pointermove", move);
        window.removeEventListener("pointerup", up);
        window.removeEventListener("pointercancel", up);
        if (!ghost) return;
        ghost.remove();
        el.classList.remove("is-dragging");
        document.body.classList.remove("block-dragging");
        over?.classList.remove("is-over");
        // The pointer comes up over the card: that isn't a click.
        el.addEventListener("click", (ev) => ev.stopPropagation(), { capture: true, once: true });
        if (over) this.drops.get(over)?.(path);
      };
      window.addEventListener("pointermove", move);
      window.addEventListener("pointerup", up);
      window.addEventListener("pointercancel", up);
    });
  }

  // ----------------------------------------------------------- calendar

  private dateKey(): string {
    if (this.cfg.date) return this.cfg.date;
    // The first property that holds dates.
    for (const r of this.q!.rows) {
      for (const [k, v] of Object.entries(r.props)) if (typeof v === "string" && /^\d{4}-\d{2}-\d{2}/.test(v)) return k;
    }
    return "date";
  }

  private calendar(rows: NoteRow[]) {
    const key = this.dateKey();
    const id = `${this.host.source}#${this.el.dataset.line ?? ""}`;
    const month = months.get(id) ?? isoDay().slice(0, 7);
    const [y, m] = month.split("-").map(Number);
    const first = new Date(y, m - 1, 1);
    const start = new Date(first);
    start.setDate(1 - first.getDay()); // weeks start on Sunday
    const byDay = new Map<string, NoteRow[]>();
    const undated: NoteRow[] = [];
    for (const r of rows) {
      const v = prop(r, key);
      const d = typeof v === "string" ? v.slice(0, 10) : "";
      if (!/^\d{4}-\d{2}-\d{2}$/.test(d)) {
        undated.push(r);
        continue;
      }
      if (!byDay.has(d)) byDay.set(d, []);
      byDay.get(d)!.push(r);
    }
    const go = (delta: number) => {
      const d = new Date(y, m - 1 + delta, 1);
      months.set(id, `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, "0")}`);
      this.render();
    };
    const nav = h(
      "div.db-cal-nav",
      null,
      h("button", { title: "Previous month", onclick: () => go(-1) }, icon("caret-left")),
      h("span.db-cal-month", null, first.toLocaleDateString(undefined, { month: "long", year: "numeric" })),
      h("button", { title: "Next month", onclick: () => go(1) }, icon("caret-right")),
      h(
        "button.db-cal-today",
        {
          onclick: () => {
            months.set(id, isoDay().slice(0, 7));
            this.render();
          },
        },
        "Today",
      ),
      h("span.db-cal-key", null, `by ${key}`),
    );
    const grid = h("div.db-cal");
    for (let i = 0; i < 7; i++) {
      const d = new Date(start);
      d.setDate(start.getDate() + i);
      grid.append(h("div.db-cal-dow", null, d.toLocaleDateString(undefined, { weekday: "short" })));
    }
    const today = isoDay();
    for (let i = 0; i < 42; i++) {
      const d = new Date(start);
      d.setDate(start.getDate() + i);
      const day = isoDay(d);
      const list = byDay.get(day) ?? [];
      const cell = h(
        `div.db-day${d.getMonth() !== m - 1 ? ".is-other" : ""}${day === today ? ".is-today" : ""}` as "div",
        null,
        h("div.db-day-head", null, h("span.db-day-n", null, String(d.getDate())), this.newButton("", { [key]: day }, "db-day-add")),
        ...list.map((r) => {
          const chip = h("div.db-event", { dataset: { path: r.path } }, this.titleLink(r, "db-event-title"));
          this.draggable(chip, r.path);
          return chip;
        }),
      );
      this.dropTarget(cell, (path) => {
        const r = this.ctx!.byPath.get(path);
        if (r) this.setValue(r, Object.keys(r.props).find((k) => k.toLowerCase() === key.toLowerCase()) ?? key, day);
      });
      grid.append(cell);
    }
    this.body.append(nav, grid);
    if (undated.length) {
      this.body.append(
        h(
          "div.db-undated",
          null,
          h("span.db-field-label", null, `No ${key}:`),
          ...undated.map((r) => {
            const chip = h("span.db-event", null, this.titleLink(r, "db-event-title"));
            this.draggable(chip, r.path);
            return chip;
          }),
        ),
      );
    }
  }

  // ------------------------------------------------------------ gallery

  private gallery(rows: NoteRow[]) {
    const cols = this.columns().filter((c) => c.expr !== "cover");
    const grid = h("div.db-gallery");
    for (const r of rows) {
      const cover = typeof r.props.cover === "string" ? r.props.cover.replace(/^\[\[|\]\]$/g, "") : "";
      const img = h("div.db-cover");
      if (cover) {
        if (/^https?:\/\//i.test(cover)) img.style.backgroundImage = `url("${cover.replace(/"/g, "%22")}")`;
        else
          void api.resolveLink(cover, r.path).then((p) => {
            if (p) img.style.backgroundImage = `url("${this.host.assetUrl(p)}")`;
          });
      } else {
        const ic = typeof r.props.icon === "string" ? r.props.icon : "";
        img.append(h("span.db-cover-icon", null, ic || r.title.slice(0, 1).toUpperCase()));
      }
      const card = this.card(r, cols, false);
      card.prepend(img);
      grid.append(card);
    }
    const add = this.newButton("New", {}, "db-new.db-gallery-new");
    grid.append(add);
    this.body.append(grid);
  }
}

/** Where a status sits in a usual workflow, for ordering board columns. */
function stage(s: string): number {
  const k = s.toLowerCase().replace(/[^a-z]/g, "");
  const order = [
    ["backlog", "idea", "ideas", "someday"],
    ["todo", "notstarted", "new", "open", "planned", "planning", "next"],
    ["inprogress", "doing", "started", "active", "wip", "ongoing"],
    ["review", "inreview", "waiting", "blocked", "onhold", "paused"],
    ["done", "complete", "completed", "finished", "closed", "shipped"],
    ["cancelled", "canceled", "dropped", "archived"],
  ];
  const i = order.findIndex((g) => g.includes(k));
  return i < 0 ? 2.5 : i;
}

function prettyDate(s: string): string {
  const d = new Date(`${s.slice(0, 10)}T00:00:00`);
  if (Number.isNaN(d.getTime())) return s;
  const out = d.toLocaleDateString(undefined, { month: "short", day: "numeric", year: d.getFullYear() === new Date().getFullYear() ? undefined : "numeric" });
  return s.length > 10 ? `${out} ${s.slice(11, 16)}` : out;
}
