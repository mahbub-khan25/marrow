// The property header: a note's frontmatter shown as editable fields under
// its title (Preview only — Source shows the YAML itself). Each field's type
// comes from its value: true/false, a number, a date, a list, links to other
// notes, or plain text. Edits go straight back into the YAML.

import type { Doc } from "./docs";
import { replaceText } from "./docs";
import { props, renameProp, setProp } from "./frontmatter";
import { showMenu } from "./menu";
import { h, icon, isoDay } from "./util";

export type Kind = "text" | "number" | "checkbox" | "date" | "list" | "link";

const KINDS: [Kind, string, string][] = [
  ["text", "Text", "text-align-left"],
  ["number", "Number", "hash-straight"],
  ["checkbox", "Checkbox", "check-square"],
  ["date", "Date", "calendar-blank"],
  ["list", "List", "list-bullets"],
  ["link", "Link to notes", "link-simple"],
];

/** Shown elsewhere (icon and cover above the title). */
const HIDDEN = new Set(["icon", "cover", "position"]);

const DATE = /^\d{4}-\d{2}-\d{2}(?:[T ]\d{2}:\d{2}(?::\d{2})?)?$/;
const LINK = /^\[\[[^\]]+\]\]$/;

export interface PropsHost {
  doc(): Doc | null;
  openLink(target: string): void;
  openTag(tag: string): void;
  /** Note names, for suggesting links. */
  noteNames(): string[];
}

/** Types picked by hand for empty values, so a new "due" stays a date.
 * Kept in the vault's settings, so they last across restarts. */
const picked = new Map<string, Kind>();
let savePicked: (types: Record<string, string>) => void = () => {};

/** Loads the vault's picked types, and says how to save changes to them. */
export function usePropertyTypes(types: Record<string, string> | undefined, save: (types: Record<string, string>) => void) {
  picked.clear();
  for (const [k, v] of Object.entries(types ?? {})) picked.set(k, v as Kind);
  savePicked = save;
}

export function kindOf(key: string, v: unknown): Kind {
  if (typeof v === "boolean") return "checkbox";
  if (typeof v === "number") return "number";
  const k = key.toLowerCase();
  if (Array.isArray(v)) return v.length && v.every((x) => typeof x === "string" && LINK.test(x.trim())) ? "link" : "list";
  if (typeof v === "string") {
    if (LINK.test(v.trim())) return "link";
    if (DATE.test(v.trim())) return "date";
    if (!v.trim()) return picked.get(key) ?? (k === "tags" || k === "aliases" ? "list" : "text");
  }
  if (v === null || v === undefined) return picked.get(key) ?? (k === "tags" || k === "aliases" ? "list" : "text");
  return "text";
}

/** A value as a list of strings. */
function asList(v: unknown): string[] {
  if (Array.isArray(v)) return v.filter((x) => x !== null && x !== undefined).map(String);
  if (v === null || v === undefined || v === "") return [];
  return [String(v)];
}

const unlink = (s: string) => s.trim().replace(/^\[\[|\]\]$/g, "").split("|")[0];

export class PropertyHeader {
  readonly el = h("div.props", { hidden: true });
  /** A key to put the cursor in after the next render. */
  private focusNext: { key: string; part: "key" | "value" } | null = null;
  private adding = false;

  constructor(private host: PropsHost) {
    // Don't redraw under the cursor; catch up when focus leaves.
    this.el.addEventListener("focusout", () => {
      window.setTimeout(() => {
        if (!this.el.contains(document.activeElement)) this.render();
      }, 0);
    });
  }

  private write(fn: (text: string) => string) {
    const doc = this.host.doc();
    if (!doc) return;
    const next = fn(doc.text);
    if (next !== doc.text) replaceText(doc, next);
    // Chips redraw at once and keep the cursor in their box.
    if (this.focusNext) this.render(true);
  }

  private set(key: string, value: unknown) {
    this.write((t) => setProp(t, key, value));
  }

  /** Starts a new property (from "Add property"). */
  add() {
    this.adding = true;
    this.render(true);
  }

  render(force = false) {
    if (!force && this.el.contains(document.activeElement)) return;
    const doc = this.host.doc();
    const p = doc ? props(doc.text) : {};
    const keys = Object.keys(p).filter((k) => !HIDDEN.has(k));
    this.el.hidden = !keys.length && !this.adding;
    if (this.el.hidden) {
      this.el.replaceChildren();
      return;
    }
    const rows = keys.map((k) => this.row(k, p[k]));
    if (this.adding) rows.push(this.newRow());
    const names = keys.some((k) => kindOf(k, p[k]) === "link")
      ? h("datalist", { id: "prop-notes" }, ...this.host.noteNames().slice(0, 2000).map((n) => h("option", { value: n })))
      : "";
    this.el.replaceChildren(
      ...rows,
      this.adding ? "" : h("button.prop-add", { onclick: () => this.add() }, icon("plus"), "Add property"),
      names,
    );
    const f = this.focusNext;
    this.focusNext = null;
    if (f) {
      const row = this.el.querySelector<HTMLElement>(`.prop-row[data-key="${CSS.escape(f.key)}"]`);
      row?.querySelector<HTMLElement>(f.part === "key" ? ".prop-key" : "input, .prop-chips input")?.focus();
    }
  }

  private newRow(): HTMLElement {
    const input = h("input.prop-name", { placeholder: "Property name", spellcheck: false }) as HTMLInputElement;
    const done = (save: boolean) => {
      const key = input.value.trim().replace(/:$/, "");
      this.adding = false;
      if (save && key && !HIDDEN.has(key)) {
        const doc = this.host.doc();
        const existing = doc ? props(doc.text) : {};
        this.focusNext = { key, part: "value" };
        if (!(key in existing)) {
          const k = key.toLowerCase();
          this.set(key, k === "tags" || k === "aliases" ? [] : "");
        }
      }
      this.render(true);
    };
    input.addEventListener("keydown", (e) => {
      if (e.key === "Enter") {
        e.preventDefault();
        done(true);
      } else if (e.key === "Escape") {
        e.preventDefault();
        done(false);
      }
    });
    input.addEventListener("blur", () => window.setTimeout(() => this.adding && done(true), 0));
    window.setTimeout(() => input.focus(), 0);
    return h("div.prop-row.is-new", null, h("span.prop-key", null, icon("plus")), input);
  }

  private row(key: string, value: unknown): HTMLElement {
    const kind = kindOf(key, value);
    const ic = KINDS.find((k) => k[0] === kind)![2];
    const keyBtn = h(
      "button.prop-key",
      {
        title: "Change type, rename or remove",
        onclick: (e: MouseEvent) => this.keyMenu(key, value, kind, e),
      },
      icon(key.toLowerCase() === "tags" ? "tag" : ic),
      h("span", null, key),
    );
    return h("div.prop-row", { dataset: { key } }, keyBtn, h("div.prop-value", null, this.editor(key, value, kind)));
  }

  private keyMenu(key: string, value: unknown, kind: Kind, e: MouseEvent) {
    showMenu(e.clientX, e.clientY, [
      ...KINDS.filter(([k]) => k !== kind).map(([k, label, ic]) => ({
        label: `Make it ${label.toLowerCase()}`,
        icon: ic,
        run: () => {
          picked.set(key, k);
          savePicked(Object.fromEntries(picked));
          this.set(key, convert(value, k));
        },
      })),
      "-",
      { label: "Rename", icon: "pencil-simple", run: () => this.rename(key) },
      { label: "Remove", icon: "trash", danger: true, run: () => this.set(key, undefined) },
    ]);
  }

  private rename(key: string) {
    const row = this.el.querySelector<HTMLElement>(`.prop-row[data-key="${CSS.escape(key)}"]`);
    const label = row?.querySelector<HTMLElement>(".prop-key span");
    if (!row || !label) return;
    const input = h("input.prop-name", { value: key, spellcheck: false }) as HTMLInputElement;
    label.replaceWith(input);
    input.focus();
    input.select();
    let finished = false;
    const done = (save: boolean) => {
      if (finished) return;
      finished = true;
      const to = input.value.trim().replace(/:$/, "");
      if (save && to && to !== key) this.write((t) => renameProp(t, key, to));
      this.render(true);
    };
    input.addEventListener("keydown", (e) => {
      e.stopPropagation();
      if (e.key === "Enter") done(true);
      else if (e.key === "Escape") done(false);
    });
    input.addEventListener("click", (e) => e.stopPropagation());
    input.addEventListener("blur", () => done(true));
  }

  private editor(key: string, value: unknown, kind: Kind): HTMLElement {
    switch (kind) {
      case "checkbox": {
        const box = h("input.prop-check", { type: "checkbox", checked: value === true }) as HTMLInputElement;
        box.addEventListener("change", () => this.set(key, box.checked));
        return box;
      }
      case "number": {
        const input = h("input.prop-input", { type: "number", value: value === null || value === undefined ? "" : String(value), placeholder: "Empty" }) as HTMLInputElement;
        input.addEventListener("change", () => this.set(key, input.value === "" ? null : Number(input.value)));
        return input;
      }
      case "date": {
        const s = typeof value === "string" ? value.trim() : "";
        const timed = s.length > 10;
        const input = h("input.prop-input.prop-date", {
          type: timed ? "datetime-local" : "date",
          value: timed ? s.replace(" ", "T").slice(0, 16) : s,
        }) as HTMLInputElement;
        input.addEventListener("change", () => this.set(key, input.value));
        return input;
      }
      case "list":
      case "link":
        return this.chips(key, asList(value), kind);
      default: {
        const input = h("input.prop-input", { value: value === null || value === undefined ? "" : String(value), placeholder: "Empty", spellcheck: false }) as HTMLInputElement;
        input.addEventListener("change", () => this.set(key, input.value));
        input.addEventListener("keydown", (e) => e.key === "Enter" && input.blur());
        return input;
      }
    }
  }

  /** Tags, aliases, lists and links: one chip per item, and a box to add one. */
  private chips(key: string, items: string[], kind: Kind): HTMLElement {
    const isTags = key.toLowerCase() === "tags";
    const save = (next: string[]) => {
      // One link stays a single value, like Obsidian writes it.
      if (kind === "link" && next.length === 1 && !Array.isArray(props(this.host.doc()?.text ?? "")[key])) this.set(key, next[0]);
      else this.set(key, next);
    };
    const chip = (item: string, i: number) => {
      const label = kind === "link" ? unlink(item) : isTags ? `#${item.replace(/^#/, "")}` : item;
      return h(
        `span.prop-chip${kind === "link" ? ".is-link" : ""}` as "span",
        null,
        h(
          "span",
          {
            onclick: () => {
              if (kind === "link") this.host.openLink(unlink(item));
              else if (isTags) this.host.openTag(item.replace(/^#/, ""));
            },
          },
          label,
        ),
        h(
          "button",
          {
            title: "Remove",
            onclick: () => save(items.filter((_, j) => j !== i)),
          },
          icon("x"),
        ),
      );
    };
    const input = h("input.prop-chip-input", {
      placeholder: items.length ? "" : kind === "link" ? "Link a note" : "Empty",
      spellcheck: false,
      list: kind === "link" ? "prop-notes" : undefined,
    }) as HTMLInputElement;
    const commit = () => {
      const raw = input.value.trim().replace(/,$/, "");
      if (!raw) return;
      const add = raw.split(/\s*,\s*/).filter(Boolean).map((s) => (kind === "link" ? `[[${unlink(s)}]]` : isTags ? s.replace(/^#/, "") : s));
      input.value = "";
      this.focusNext = { key, part: "value" };
      save([...items, ...add]);
    };
    input.addEventListener("keydown", (e) => {
      if (e.key === "Enter" || (e.key === "," && kind !== "link")) {
        e.preventDefault();
        commit();
      } else if (e.key === "Backspace" && !input.value && items.length) {
        this.focusNext = { key, part: "value" };
        save(items.slice(0, -1));
      }
    });
    input.addEventListener("change", () => {
      // Picking from the suggestions.
      if (kind === "link" && this.host.noteNames().includes(input.value.trim())) commit();
    });
    input.addEventListener("blur", commit);
    return h("div.prop-chips", null, ...items.map(chip), input);
  }
}

function convert(v: unknown, to: Kind): unknown {
  const list = asList(v);
  switch (to) {
    case "checkbox":
      return v === true || v === "true" || v === "yes";
    case "number": {
      const n = Number(list[0]);
      return Number.isFinite(n) && list.length ? n : null;
    }
    case "date":
      return typeof v === "string" && DATE.test(v.trim()) ? v : isoDay();
    case "list":
      return list.map(unlink);
    case "link":
      return list.length === 1 ? `[[${unlink(list[0])}]]` : list.map((s) => `[[${unlink(s)}]]`);
    default:
      return list.map((s) => (LINK.test(s) ? s : s)).join(", ");
  }
}
