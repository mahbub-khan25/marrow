// Editing Markdown tables in Preview: click a cell and type; Tab and
// Shift+Tab move between cells, Enter goes down; right-click a cell for row
// and column actions; the + bars add a row or a column. The table is
// written back as a tidy Markdown table.

import type { Doc } from "./docs";
import { replaceText } from "./docs";
import { showMenu } from "./menu";
import { h, icon } from "./util";

type Align = "" | "left" | "center" | "right";

export interface TableModel {
  /** First and after-last line in the file. */
  start: number;
  end: number;
  indent: string;
  /** Row 0 is the header. */
  rows: string[][];
  align: Align[];
}

const isRow = (l: string) => /^\s*\|/.test(l) || /[^\\]\|/.test(l);
const SEP = /^\s*\|?\s*:?-{1,}:?\s*(\|\s*:?-{1,}:?\s*)*\|?\s*$/;

/** Splits a table line into cells (a `\|` stays inside its cell). */
export function splitRow(line: string): string[] {
  let s = line.trim();
  if (s.startsWith("|")) s = s.slice(1);
  if (s.endsWith("|") && !s.endsWith("\\|")) s = s.slice(0, -1);
  const cells: string[] = [];
  let cur = "";
  let code = false;
  for (let i = 0; i < s.length; i++) {
    const c = s[i];
    if (c === "\\" && s[i + 1] === "|") {
      cur += "\\|";
      i++;
    } else if (c === "`") {
      code = !code;
      cur += c;
    } else if (c === "|" && !code) {
      cells.push(cur.trim());
      cur = "";
    } else cur += c;
  }
  cells.push(cur.trim());
  return cells;
}

export function parseTable(lines: string[], start: number): TableModel | null {
  if (!isRow(lines[start] ?? "") || !SEP.test(lines[start + 1] ?? "")) return null;
  let end = start + 2;
  while (end < lines.length && lines[end].trim() && isRow(lines[end])) end++;
  const indent = /^\s*/.exec(lines[start])![0];
  const header = splitRow(lines[start]);
  const align = splitRow(lines[start + 1]).map((c): Align => {
    const l = c.startsWith(":");
    const r = c.endsWith(":");
    return l && r ? "center" : r ? "right" : l ? "left" : "";
  });
  const rows = [header, ...lines.slice(start + 2, end).map(splitRow)];
  const width = Math.max(...rows.map((r) => r.length), align.length);
  for (const r of rows) while (r.length < width) r.push("");
  while (align.length < width) align.push("");
  return { start, end, indent, rows, align };
}

/** Wide characters (CJK, emoji) take two columns in a monospace editor. */
function width(s: string): number {
  let w = 0;
  for (const ch of s) w += /[ᄀ-ᅟ⺀-꓏가-힣豈-﫿︰-﹏＀-｠￠-￦]|\p{Extended_Pictographic}/u.test(ch) ? 2 : 1;
  return w;
}

export function serialize(t: TableModel): string[] {
  const cols = t.align.length;
  const w = Array.from({ length: cols }, (_, c) => Math.max(3, ...t.rows.map((r) => width(r[c] ?? ""))));
  const pad = (s: string, c: number) => {
    const extra = w[c] - width(s);
    if (t.align[c] === "right") return " ".repeat(extra) + s;
    if (t.align[c] === "center") return " ".repeat(Math.floor(extra / 2)) + s + " ".repeat(Math.ceil(extra / 2));
    return s + " ".repeat(extra);
  };
  const line = (r: string[]) => `${t.indent}| ${r.map((s, c) => pad(s, c)).join(" | ")} |`;
  const sep = `${t.indent}|${t.align
    .map((a, c) => {
      const dashes = "-".repeat(w[c]);
      if (a === "center") return ` :${dashes.slice(2)}: `;
      if (a === "right") return ` ${dashes.slice(1)}: `;
      if (a === "left") return ` :${dashes.slice(1)} `;
      return ` ${dashes} `;
    })
    .join("|")}|`;
  return [line(t.rows[0]), sep, ...t.rows.slice(1).map(line)];
}

/** What a person typed, made safe for a table cell. */
function cellText(s: string): string {
  return s
    .replace(/\r?\n/g, " ")
    .replace(/(^|[^\\])\|/g, "$1\\|")
    .trim();
}

export interface TableHost {
  doc(): Doc | null;
  preview: HTMLElement;
  /** Draws the preview again even though the text didn't change. */
  rerender(): void;
}

interface Pending {
  start: number;
  row: number;
  col: number;
}

export class TableEditor {
  /** A cell to open once the preview has redrawn. */
  private pending: Pending | null = null;
  /** Ends the cell being edited, then opens `next` (any table). */
  private active: ((next?: Pending) => void) | null = null;

  constructor(private host: TableHost) {
    // Clicking another cell while editing: save, then open that one after the redraw.
    host.preview.addEventListener(
      "mousedown",
      (e) => {
        if (!this.active) return;
        const cell = (e.target as HTMLElement).closest<HTMLElement>("td, th");
        const table = cell && this.tableOf(cell);
        if (!cell || !table || cell.isContentEditable) return;
        e.preventDefault();
        this.active({ start: Number(table.dataset.line), ...this.pos(cell) });
      },
      true,
    );
    host.preview.addEventListener("click", (e) => {
      const t = e.target as HTMLElement;
      if (t.closest("a, [data-tag], .tbl-add")) return;
      const cell = t.closest<HTMLElement>("td, th");
      if (cell && !cell.isContentEditable && this.tableOf(cell)) this.open(cell);
    });
    host.preview.addEventListener("contextmenu", (e) => {
      const cell = (e.target as HTMLElement).closest<HTMLElement>("td, th");
      if (!cell || !this.tableOf(cell)) return;
      e.preventDefault();
      this.menu(cell, e.clientX, e.clientY);
    });
  }

  /** After each render: + bars, and reopen the cell being edited. */
  decorate() {
    for (const table of this.host.preview.querySelectorAll<HTMLTableElement>(":scope > table[data-line]")) {
      const wrap = h("div.tbl-wrap");
      table.replaceWith(wrap);
      wrap.append(
        table,
        h("button.tbl-add.tbl-add-row", { title: "Add a row", onclick: () => this.addRow(table) }, icon("plus")),
        h("button.tbl-add.tbl-add-col", { title: "Add a column", onclick: () => this.addCol(table) }, icon("plus")),
      );
      wrap.dataset.line = table.dataset.line;
    }
    const p = this.pending;
    this.pending = null;
    if (p) {
      const table = this.host.preview.querySelector<HTMLTableElement>(`table[data-line="${p.start}"]`);
      const cell = table ? this.cellAt(table, p.row, p.col) : null;
      if (cell) this.open(cell);
    }
  }

  private tableOf(cell: HTMLElement): HTMLTableElement | null {
    const t = cell.closest<HTMLTableElement>("table[data-line]");
    return t && !t.closest(".embed") ? t : null;
  }

  private model(table: HTMLTableElement): TableModel | null {
    const doc = this.host.doc();
    if (!doc) return null;
    return parseTable(doc.text.split("\n"), Number(table.dataset.line));
  }

  private pos(cell: HTMLElement): { row: number; col: number } {
    const tr = cell.parentElement as HTMLTableRowElement;
    const inHead = tr.parentElement?.tagName === "THEAD";
    const body = tr.closest("table")!.tBodies[0];
    const row = inHead ? 0 : 1 + Array.from(body?.rows ?? []).indexOf(tr);
    return { row, col: (cell as HTMLTableCellElement).cellIndex };
  }

  private cellAt(table: HTMLTableElement, row: number, col: number): HTMLElement | null {
    const tr = row === 0 ? table.tHead?.rows[0] : table.tBodies[0]?.rows[row - 1];
    return (tr?.cells[col] as HTMLElement | undefined) ?? null;
  }

  private write(t: TableModel, focus?: { row: number; col: number }) {
    const doc = this.host.doc();
    if (!doc) return;
    const lines = doc.text.split("\n");
    lines.splice(t.start, t.end - t.start, ...serialize(t));
    const next = lines.join("\n");
    if (focus) this.pending = { start: t.start, ...focus };
    if (next !== doc.text) replaceText(doc, next);
    else if (focus) this.decorate();
  }

  /** Edit a cell in place, as its Markdown. */
  private open(cell: HTMLElement) {
    const table = this.tableOf(cell);
    const t = table && this.model(table);
    if (!table || !t) return;
    const { row, col } = this.pos(cell);
    const raw = t.rows[row]?.[col] ?? "";
    cell.textContent = raw.replace(/\\\|/g, "|");
    cell.contentEditable = "plaintext-only";
    cell.classList.add("is-editing");
    cell.focus();
    const sel = window.getSelection();
    const r = document.createRange();
    r.selectNodeContents(cell);
    r.collapse(false);
    sel?.removeAllRanges();
    sel?.addRange(r);

    let done = false;
    const finish = (go?: { row: number; col: number }, other?: Pending) => {
      if (done) return;
      done = true;
      this.active = null;
      cell.removeEventListener("blur", onBlur);
      const fresh = this.model(table);
      if (!fresh) return;
      const value = cellText(cell.textContent ?? "");
      const target = go && go.row >= 0 && go.row < fresh.rows.length && go.col >= 0 && go.col < fresh.align.length ? go : undefined;
      if (fresh.rows[row][col] !== value) {
        fresh.rows[row][col] = value;
        this.write(fresh, target);
        if (other) this.pending = other;
      } else {
        if (other) this.pending = other;
        // Nothing changed: draw the cell as before, and move on if asked.
        if (target) this.pending = { start: fresh.start, ...target };
        this.host.rerender();
        if (!target && !other) return;
      }
    };
    const onBlur = () => finish();
    this.active = (next) => finish(undefined, next);
    cell.addEventListener("blur", onBlur);
    cell.addEventListener("keydown", (e) => {
      const cols = t.align.length;
      if (e.key === "Tab") {
        e.preventDefault();
        let r2 = row;
        let c2 = col + (e.shiftKey ? -1 : 1);
        if (c2 >= cols) {
          c2 = 0;
          r2++;
        } else if (c2 < 0) {
          c2 = cols - 1;
          r2--;
        }
        if (r2 >= t.rows.length && !e.shiftKey) {
          // Tab past the last cell adds a row, like a spreadsheet.
          const fresh = this.model(table);
          if (fresh) {
            fresh.rows[row][col] = cellText(cell.textContent ?? "");
            fresh.rows.push(Array(cols).fill(""));
            done = true;
            this.write(fresh, { row: r2, col: 0 });
          }
          return;
        }
        finish({ row: r2, col: c2 });
      } else if (e.key === "Enter") {
        e.preventDefault();
        finish({ row: row + 1, col });
      } else if (e.key === "Escape") {
        e.preventDefault();
        cell.textContent = raw.replace(/\\\|/g, "|");
        cell.blur();
      }
    });
  }

  private addRow(table: HTMLTableElement, at?: number) {
    const t = this.model(table);
    if (!t) return;
    const i = at ?? t.rows.length;
    t.rows.splice(i, 0, Array(t.align.length).fill(""));
    this.write(t, { row: i, col: 0 });
  }

  private addCol(table: HTMLTableElement, at?: number) {
    const t = this.model(table);
    if (!t) return;
    const c = at ?? t.align.length;
    t.align.splice(c, 0, "");
    t.rows.forEach((r, i) => r.splice(c, 0, i === 0 ? "Column" : ""));
    this.write(t, { row: 0, col: c });
  }

  private menu(cell: HTMLElement, x: number, y: number) {
    const table = this.tableOf(cell)!;
    const t = this.model(table);
    if (!t) return;
    const { row, col } = this.pos(cell);
    const edit = (fn: (m: TableModel) => void) => () => {
      const m = this.model(table);
      if (!m) return;
      fn(m);
      this.write(m);
    };
    const sortBy = (dir: 1 | -1) =>
      edit((m) => {
        const body = m.rows.slice(1);
        const num = body.every((r) => r[col] === "" || !Number.isNaN(Number(r[col])));
        body.sort((a, b) => dir * (num ? Number(a[col] || 0) - Number(b[col] || 0) : a[col].localeCompare(b[col], undefined, { numeric: true, sensitivity: "base" })));
        m.rows = [m.rows[0], ...body];
      });
    showMenu(x, y, [
      ...(row > 0 ? [{ label: "Insert row above", icon: "arrow-line-up", run: () => this.addRow(table, row) }] : []),
      { label: "Insert row below", icon: "arrow-line-down", run: () => this.addRow(table, row + 1) },
      { label: "Insert column left", icon: "arrow-line-left", run: () => this.addCol(table, col) },
      { label: "Insert column right", icon: "arrow-line-right", run: () => this.addCol(table, col + 1) },
      "-",
      { label: "Sort A → Z", icon: "sort-ascending", run: sortBy(1) },
      { label: "Sort Z → A", icon: "sort-descending", run: sortBy(-1) },
      { label: "Align left", icon: "text-align-left", run: edit((m) => (m.align[col] = "left")) },
      { label: "Align centre", icon: "text-align-center", run: edit((m) => (m.align[col] = "center")) },
      { label: "Align right", icon: "text-align-right", run: edit((m) => (m.align[col] = "right")) },
      "-",
      ...(row > 0 && t.rows.length > 2
        ? [{ label: "Delete row", icon: "trash", danger: true, run: edit((m) => void m.rows.splice(row, 1)) }]
        : []),
      ...(t.align.length > 1
        ? [
            {
              label: "Delete column",
              icon: "trash",
              danger: true,
              run: edit((m) => {
                m.align.splice(col, 1);
                for (const r of m.rows) r.splice(col, 1);
              }),
            },
          ]
        : []),
    ]);
  }
}
