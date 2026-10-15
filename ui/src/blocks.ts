// Drag handles in Preview. Hover a block (paragraph, heading, list item,
// callout, table…) and a handle appears in the margin: drag it to move the
// block, or click it for Duplicate, Move up/down and Delete. Moving a block
// moves its Markdown lines, so the file stays plain Markdown.

import type { Doc } from "./docs";
import { replaceText } from "./docs";
import { showMenu, type MenuItem } from "./menu";
import { h, icon } from "./util";

export interface Unit {
  el: HTMLElement;
  /** First line (0-based, in the whole file). */
  start: number;
  /** Line after the last one. */
  end: number;
  li: boolean;
}

export interface BlockHost {
  doc(): Doc | null;
  preview: HTMLElement;
  /** The positioned column the handle lives in. */
  col: HTMLElement;
  /** More menu items for a block (synced blocks add one). */
  menuExtras?(u: Unit, lines: string[]): MenuItem[];
}

/** Blocks in reading order: top-level blocks, and each item of a top-level list. */
export function units(preview: HTMLElement, lines: string[]): Unit[] {
  const out: Unit[] = [];
  for (const child of Array.from(preview.children) as HTMLElement[]) {
    if (child.dataset.line === undefined) continue;
    if (child.tagName === "UL" || child.tagName === "OL") {
      for (const li of Array.from(child.children) as HTMLElement[]) {
        if (li.tagName === "LI" && li.dataset.line !== undefined) out.push({ el: li, start: Number(li.dataset.line), end: 0, li: true });
      }
    } else {
      out.push({ el: child, start: Number(child.dataset.line), end: 0, li: false });
    }
  }
  for (let i = 0; i < out.length; i++) {
    let end = i + 1 < out.length ? out[i + 1].start : lines.length;
    const hm = /^H([1-6])$/.exec(out[i].el.tagName);
    // A collapsed heading carries its hidden section with it.
    if (hm && out[i].el.classList.contains("is-collapsed")) {
      const level = Number(hm[1]);
      const next = out.slice(i + 1).find((u) => {
        const m = /^H([1-6])$/.exec(u.el.tagName);
        return m && Number(m[1]) <= level;
      });
      end = next ? next.start : lines.length;
    }
    while (end > out[i].start + 1 && (lines[end - 1] ?? "").trim() === "") end--;
    out[i].end = Math.max(end, out[i].start + 1);
  }
  return out;
}

/** Moves lines `s..e` so they start where line `target` is now. */
export function moveLines(text: string, s: number, e: number, target: number, pad: boolean): string {
  if (target >= s && target <= e) return text;
  const lines = text.split("\n");
  const block = lines.slice(s, e);
  lines.splice(s, e - s);
  let t = target > e ? target - (e - s) : target;
  // Don't leave two blank lines where the block was.
  if (s > 0 && s < lines.length && lines[s - 1].trim() === "" && lines[s].trim() === "") {
    lines.splice(s, 1);
    if (t > s) t--;
  }
  const ins = [...block];
  if (pad) {
    if (t > 0 && (lines[t - 1] ?? "").trim() !== "") ins.unshift("");
    if (t < lines.length && lines[t].trim() !== "") ins.push("");
  }
  lines.splice(t, 0, ...ins);
  return lines.join("\n");
}

export class BlockHandles {
  private handle = h("button.block-handle", { title: "Drag to move · click for more", "aria-label": "Block handle", tabindex: "-1" }, icon("dots-six-vertical"));
  private dropLine = h("div.block-drop", { hidden: true });
  private current: Unit | null = null;
  private list: Unit[] = [];
  private dragging = false;

  constructor(private host: BlockHost) {
    this.handle.hidden = true;
    host.col.append(this.handle, this.dropLine);
    host.col.addEventListener("mousemove", (e) => {
      if (!this.dragging) this.hover(e.clientY, e.target as HTMLElement);
    });
    host.col.addEventListener("mouseleave", () => {
      if (!this.dragging) this.hide();
    });
    this.handle.addEventListener("pointerdown", (e) => this.startDrag(e));
  }

  hide() {
    this.handle.hidden = true;
    this.current = null;
  }

  private lines(): string[] {
    return (this.host.doc()?.text ?? "").split("\n");
  }

  private hover(y: number, target: HTMLElement) {
    const { preview } = this.host;
    if (preview.hidden || target.closest(".props, .note-head, .note-title, .note-meta, .icon-pop, input, textarea, .embed.is-editing, [contenteditable='true'], [contenteditable='plaintext-only']")) {
      if (target !== this.handle && !this.handle.contains(target)) this.hide();
      return;
    }
    if (target === this.handle || this.handle.contains(target)) return;
    this.list = units(preview, this.lines());
    const u = this.list.find((x) => {
      if (x.el.classList.contains("is-hidden")) return false;
      const r = x.el.getBoundingClientRect();
      return y >= r.top - 4 && y <= r.bottom + 4;
    });
    if (!u) {
      this.hide();
      return;
    }
    this.current = u;
    const col = this.host.col.getBoundingClientRect();
    const r = u.el.getBoundingClientRect();
    const lh = parseFloat(getComputedStyle(u.el).lineHeight) || 28;
    this.handle.hidden = false;
    this.handle.style.left = `${r.left - col.left - (u.li ? 58 : 52)}px`;
    this.handle.style.top = `${r.top - col.top + Math.max(0, (Math.min(lh, r.height) - 24) / 2)}px`;
  }

  /** Where a drop at `y` would go: before unit i, or at the end. */
  private dropAt(y: number): { index: number; top: number; left: number; width: number } {
    const col = this.host.col.getBoundingClientRect();
    const visible = this.list.filter((u) => !u.el.classList.contains("is-hidden"));
    for (const u of visible) {
      const r = u.el.getBoundingClientRect();
      if (y < r.top + r.height / 2) {
        return { index: this.list.indexOf(u), top: r.top - col.top - 3, left: r.left - col.left, width: r.width };
      }
    }
    const last = visible[visible.length - 1];
    const r = last.el.getBoundingClientRect();
    return { index: this.list.length, top: r.bottom - col.top + 3, left: r.left - col.left, width: r.width };
  }

  private startDrag(e: PointerEvent) {
    const u = this.current;
    if (!u || e.button !== 0) return;
    e.preventDefault();
    const x0 = e.clientX;
    const y0 = e.clientY;
    let moved = false;
    let drop: ReturnType<BlockHandles["dropAt"]> | null = null;
    this.handle.setPointerCapture(e.pointerId);
    const move = (ev: PointerEvent) => {
      if (!moved && Math.hypot(ev.clientX - x0, ev.clientY - y0) < 5) return;
      if (!moved) {
        moved = true;
        this.dragging = true;
        u.el.classList.add("is-dragging");
        document.body.classList.add("block-dragging");
      }
      drop = this.dropAt(ev.clientY);
      this.dropLine.hidden = false;
      Object.assign(this.dropLine.style, { top: `${drop.top}px`, left: `${drop.left}px`, width: `${drop.width}px` });
      // Scroll when dragging near the edges.
      const sc = this.host.col.parentElement;
      if (sc) {
        const r = sc.getBoundingClientRect();
        if (ev.clientY < r.top + 40) sc.scrollTop -= 12;
        else if (ev.clientY > r.bottom - 40) sc.scrollTop += 12;
      }
    };
    const up = () => {
      this.handle.removeEventListener("pointermove", move);
      this.handle.removeEventListener("pointerup", up);
      this.handle.removeEventListener("pointercancel", up);
      this.dragging = false;
      this.dropLine.hidden = true;
      u.el.classList.remove("is-dragging");
      document.body.classList.remove("block-dragging");
      if (!moved) {
        const r = this.handle.getBoundingClientRect();
        this.menu(u, r.left, r.bottom + 4);
        return;
      }
      if (drop) this.moveTo(u, drop.index);
      this.hide();
    };
    this.handle.addEventListener("pointermove", move);
    this.handle.addEventListener("pointerup", up);
    this.handle.addEventListener("pointercancel", up);
  }

  private moveTo(u: Unit, index: number) {
    const doc = this.host.doc();
    if (!doc) return;
    const lines = doc.text.split("\n");
    const target = index < this.list.length ? this.list[index].start : lines.length;
    const before = this.list[index - 1];
    const after = this.list[index];
    // List items dropped among list items stay tight; anything else gets blank lines around it.
    const pad = !(u.li && (after?.li || before?.li));
    const next = moveLines(doc.text, u.start, u.end, target, pad);
    if (next !== doc.text) replaceText(doc, next);
  }

  private menu(u: Unit, x: number, y: number) {
    const doc = this.host.doc();
    if (!doc) return;
    const i = this.list.indexOf(u);
    const lines = doc.text.split("\n");
    const items: MenuItem[] = [
      {
        label: "Duplicate",
        icon: "copy",
        run: () => {
          const ls = doc.text.split("\n");
          const copy = ls.slice(u.start, u.end);
          ls.splice(u.end, 0, ...(u.li ? copy : ["", ...copy]));
          replaceText(doc, ls.join("\n"));
        },
      },
      ...(i > 0 ? [{ label: "Move up", icon: "arrow-up", run: () => this.moveTo(u, i - 1) }] : []),
      ...(i < this.list.length - 1 ? [{ label: "Move down", icon: "arrow-down", run: () => this.moveTo(u, i + 2) }] : []),
      ...(this.host.menuExtras?.(u, lines) ?? []),
      "-",
      {
        label: "Delete",
        icon: "trash",
        danger: true,
        run: () => {
          const ls = doc.text.split("\n");
          ls.splice(u.start, u.end - u.start);
          // Don't leave a double blank line behind.
          if (u.start > 0 && (ls[u.start - 1] ?? "").trim() === "" && (ls[u.start] ?? "").trim() === "") ls.splice(u.start, 1);
          replaceText(doc, ls.join("\n"));
        },
      },
    ];
    showMenu(x, y, items);
  }
}
