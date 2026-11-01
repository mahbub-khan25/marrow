// The Home board: the first thing Marrow shows, and the person's own place.
// It's a canvas (`Home.canvas` in the vault, so it syncs and has history)
// whose cards can be live: sticky notes, tasks picked by any filter, a note
// or one part of it, lists of notes, a greeting, a quick-note box, a
// calendar, today's note, a countdown, a clock, shortcuts and "rediscover".
// Everything can be moved, resized, restyled and removed; pictures, links,
// text, shapes and drawings can go on it too. Boards can be saved as
// templates (a `.marrowhome` file) to share, and templates imported.
//
// File format (shared with the phone apps). A live card is a JSON Canvas
// text node with Marrow's extra fields:
//   widget: "sticky" | "tasks" | "note" | "notes" | "greeting" | "capture" |
//           "calendar" | "daily" | "countdown" | "clock" | "shortcuts" |
//           "rediscover"
//   cfg:    the card's settings (see WIDGETS below)
//   style:  { bg, tint, fg, font, size, look, radius, opacity, align }
//   text:   a sticky's words; for other cards a line that says what it is
//           (what other apps show)
// and the file's `home` field holds the board's own settings:
//   { bg: "dots" | "grid" | "lines" | "paper" | "plain" | "cork", color, image }

import { convertFileSrc } from "@tauri-apps/api/core";
import { save as saveDialog } from "@tauri-apps/plugin-dialog";

import { api, displayName, type TaskFilter, type TaskItem } from "./api";
import { CanvasView, type CanvasApp, type CData, type CNode } from "./canvas";
import { renderMarkdown } from "./preview";
import { renderResurface, type ResurfaceApp } from "./resurface";
import { toggleTaskAt } from "./tasks";
import { h, icon, isoDay, plainText, trunc } from "./util";
import { uiLocale } from "./i18n";
import { prefs } from "./prefs";

export const HOME_FILE = "Home.canvas";

export interface HomeApp extends CanvasApp {
  /** Every file in the vault (paths). */
  files(): string[];
  starred(): string[];
  resurface: ResurfaceApp;
  openToday(): void;
  openDaily(day: string): void;
  appendToToday(line: string, section: "Log" | "Tasks"): Promise<void>;
  appendToNote(path: string, line: string): Promise<void>;
  newNote(): void;
  search(): void;
  ask(): void;
  capture(): void;
  moveFrom(kind: "notion" | "evernote"): void;
  importNotes(): void;
  noteCount(): number;
}

// ------------------------------------------------------------------ cards

type Cfg = Record<string, unknown>;

interface WidgetDef {
  name: string;
  icon: string;
  w: number;
  h: number;
  cfg: Cfg;
  style?: Cfg;
  /** What other apps show for it. */
  label: (n: CNode) => string;
}

const TASKS_DEFAULT: Cfg = { title: "Next 7 days", range: "days", days: 7, from: "", to: "", overdue: true, undated: false, done: false, tag: "", folder: "", note: "", text: "", group: true, showNote: true };

export const WIDGETS: Record<string, WidgetDef> = {
  sticky: { name: "Sticky note", icon: "note", w: 240, h: 200, cfg: {}, style: { bg: "#ffd84d", tint: 45, look: "paper" }, label: (n) => n.text ?? "" },
  tasks: { name: "Tasks", icon: "check-square", w: 380, h: 360, cfg: TASKS_DEFAULT, label: (n) => `☑ ${str(n.cfg?.title) || "Tasks"} (Marrow Home card)` },
  note: { name: "A note", icon: "file-text", w: 360, h: 320, cfg: { path: "", section: "" }, label: (n) => `📄 ${str(n.cfg?.path) ? `[[${displayName(str(n.cfg?.path))}]]` : "A note"} (Marrow Home card)` },
  notes: { name: "List of notes", icon: "list-bullets", w: 300, h: 300, cfg: { title: "Recently edited", kind: "recent", value: "", limit: 8 }, label: (n) => `📚 ${str(n.cfg?.title) || "Notes"} (Marrow Home card)` },
  greeting: { name: "Greeting", icon: "hand-waving", w: 480, h: 130, cfg: { name: "", date: true, stats: true }, style: { look: "none" }, label: () => "👋 Greeting (Marrow Home card)" },
  capture: { name: "Quick note", icon: "lightning", w: 440, h: 96, cfg: { target: "daily", path: "", task: false }, label: () => "⚡ Quick note (Marrow Home card)" },
  shortcuts: { name: "Shortcuts", icon: "squares-four", w: 480, h: 70, cfg: { items: ["new", "today", "search", "ask"] }, style: { look: "none" }, label: () => "🔘 Shortcuts (Marrow Home card)" },
  calendar: { name: "Calendar", icon: "calendar-dots", w: 300, h: 300, cfg: {}, label: () => "📅 Calendar (Marrow Home card)" },
  daily: { name: "Today's note", icon: "sun", w: 360, h: 320, cfg: {}, label: () => "☀ Today's note (Marrow Home card)" },
  countdown: { name: "Countdown", icon: "hourglass-medium", w: 240, h: 140, cfg: { title: "", date: "" }, label: (n) => `⏳ ${str(n.cfg?.title) || "Countdown"} ${str(n.cfg?.date)} (Marrow Home card)` },
  clock: { name: "Clock", icon: "clock", w: 220, h: 110, cfg: { h24: false }, label: () => "🕒 Clock (Marrow Home card)" },
  welcome: { name: "Getting started", icon: "signpost", w: 520, h: 300, cfg: {}, label: () => "👋 Getting started (Marrow Home card)" },
  rediscover: { name: "Rediscover", icon: "clock-counter-clockwise", w: 360, h: 280, cfg: {}, label: () => "🔁 Rediscover (Marrow Home card)" },
};

const str = (v: unknown) => (typeof v === "string" ? v : "");
const num = (v: unknown, d: number) => (typeof v === "number" && Number.isFinite(v) ? v : d);
const newId = () => Array.from(crypto.getRandomValues(new Uint8Array(8)), (b) => b.toString(16).padStart(2, "0")).join("");

/** A new live card of `kind`, at (x, y) if given. */
export function makeCard(kind: string, at?: { x: number; y: number }, extra: Partial<CNode> = {}): CNode {
  const d = WIDGETS[kind];
  const n: CNode = {
    id: newId(),
    type: "text",
    x: at?.x ?? NaN,
    y: at?.y ?? NaN,
    width: d.w,
    height: d.h,
    widget: kind,
    cfg: structuredClone(d.cfg),
    ...(d.style ? { style: structuredClone(d.style) } : {}),
    ...extra,
  };
  if (kind !== "sticky") n.text = d.label(n);
  else n.text ??= "";
  return n;
}

// ----------------------------------------------------------------- looks

const PALETTE: [string, string][] = [
  ["Red", "#e03e3e"],
  ["Orange", "#e8780c"],
  ["Yellow", "#ffd84d"],
  ["Green", "#2f9e44"],
  ["Teal", "#0b8cb5"],
  ["Purple", "#8e44ad"],
  ["Pink", "#e64980"],
  ["Grey", "#868e96"],
];
const CANVAS_COLORS: Record<string, string> = { "1": "#e03e3e", "2": "#e8780c", "3": "#d4a106", "4": "#2f9e44", "5": "#0b8cb5", "6": "#8e44ad" };

export const LOOKS: [string, string][] = [
  ["card", "Card"],
  ["paper", "Sticky paper"],
  ["lined", "Lined paper"],
  ["grid", "Squared paper"],
  ["tape", "Taped on"],
  ["pin", "Pinned"],
  ["fold", "Folded corner"],
  ["outline", "Outline"],
  ["glass", "Frosted glass"],
  ["none", "No background"],
];
export const FONTS: [string, string][] = [
  ["", "App font"],
  ["read", "Book"],
  ["classic", "Classic"],
  ["mono", "Typewriter"],
  ["hand", "Handwritten"],
];
const SIZES: [string, string][] = [
  ["s", "Small"],
  ["", "Normal"],
  ["l", "Large"],
  ["xl", "Huge"],
];

/** Gives a card its look from `style` (any card on the board, live or not). */
export function applyStyle(n: CNode, el: HTMLElement) {
  const s = (n.style ?? {}) as Cfg;
  const bg = str(s.bg);
  const color = CANVAS_COLORS[bg] ?? (/^#[0-9a-f]{3,8}$/i.test(bg) ? bg : "");
  for (const c of [...el.classList]) if (c.startsWith("look-") || c.startsWith("size-") || c.startsWith("font-")) el.classList.remove(c);
  if (!n.style) return;
  el.classList.add(`look-${str(s.look) || "card"}`);
  if (str(s.size)) el.classList.add(`size-${str(s.size)}`);
  if (str(s.font)) el.classList.add(`font-${str(s.font)}`);
  el.classList.toggle("has-bg", !!color);
  if (color) el.style.setProperty("--w-bg", color);
  else el.style.removeProperty("--w-bg");
  el.style.setProperty("--w-tint", `${num(s.tint, 18)}%`);
  if (/^#[0-9a-f]{3,8}$/i.test(str(s.fg))) el.style.setProperty("--w-fg", str(s.fg));
  else el.style.removeProperty("--w-fg");
  el.style.setProperty("--w-radius", `${num(s.radius, 12)}px`);
  el.style.opacity = String(num(s.opacity, 100) / 100);
  el.style.textAlign = str(s.align) || "";
}

// ------------------------------------------------------------- the board

export class HomeView {
  readonly el: HTMLElement;
  private board: HTMLElement;
  private view: CanvasView;
  private panel: HTMLElement;
  private panelOpen = false;
  private loaded = false;
  private tasks: TaskItem[] | null = null;
  private month = new Map<string, string>();

  constructor(private app: HomeApp) {
    this.board = h("div.canvas-view.home-board");
    this.panel = h("aside.home-panel", { hidden: true });
    const bar = h(
      "div.home-bar",
      null,
      this.menuButton("plus", "Add", () => this.addMenu()),
      this.menuButton("paint-brush", "Customize", () => this.togglePanel()),
      this.menuButton("layout", "Templates", () => this.templateMenu()),
    );
    this.el = h("div.home", null, this.board, bar, this.panel);
    this.view = new CanvasView(this.board, app, {
      home: {
        render: (n, el, changed) => this.render(n, el, changed),
        style: (n, el) => applyStyle(n, el),
        onSelect: () => this.syncPanel(),
        onChange: () => this.applyBoard(),
      },
    });
  }

  private menuButton(ic: string, label: string, run: (e: MouseEvent) => void) {
    return h("button.home-btn", { type: "button", onclick: run }, icon(ic), h("span", null, label));
  }

  /** Opens (or creates) the board; again after switching vaults. */
  async show() {
    if (this.loaded && this.view.path) {
      this.refresh();
      return;
    }
    const exists = await api.readNote(HOME_FILE).then(
      () => true,
      () => false,
    );
    if (!exists) {
      const board = starterBoard();
      // A new, nearly empty vault: how to start, and how to bring notes over.
      if (this.app.noteCount() < 15) board.nodes.push(makeCard("welcome", { x: 0, y: 740 }));
      await api.createFile("", "Home", "canvas", JSON.stringify(board, null, "\t")).catch(() => {});
    }
    try {
      await this.view.load(HOME_FILE);
      this.loaded = true;
      this.applyBoard();
    } catch (e) {
      this.app.toast(`Couldn't open Home: ${e}`, "error");
    }
  }

  /** A new vault: start again. */
  reset() {
    this.loaded = false;
    this.view.path = null;
    this.tasks = null;
  }

  /** Notes or tasks changed: live cards catch up. */
  refresh() {
    if (!this.loaded) return;
    this.tasks = null;
    this.view.refreshLive();
  }

  async flush() {
    await this.view.flush();
  }

  /** Home.canvas changed on disk (sync, another window). */
  diskChanged() {
    if (this.loaded) void this.view.onDiskChanged();
  }

  // ----------------------------------------------------------- the board

  private applyBoard() {
    const b = this.view.board;
    const bg = str(b.bg) || "dots";
    this.board.dataset.bg = bg;
    const color = str(b.color);
    if (/^#[0-9a-f]{3,8}$/i.test(color)) this.board.style.setProperty("--home-bg", color);
    else this.board.style.removeProperty("--home-bg");
    const img = str(b.image);
    this.board.style.setProperty("--home-image", img ? `url("${this.assetUrl(img)}")` : "none");
    this.board.classList.toggle("has-image", !!img);
    if (this.panelOpen && this.view.selection.length === 0) this.syncPanel();
  }

  private assetUrl(path: string) {
    const root = this.app.vaultRoot();
    const sep = root.includes("\\") ? "\\" : "/";
    return convertFileSrc(root.replace(/[\\/]+$/, "") + sep + path.split("/").join(sep));
  }

  // ------------------------------------------------------------- add menu

  private addMenu() {
    const items: [string, string, () => void][] = Object.entries(WIDGETS).map(([k, d]) => [d.icon, d.name, () => this.add(makeCard(k))]);
    items.push(
      ["image", "Picture", () => void this.addPicture()],
      ["link-simple", "Link", () => this.addLink()],
    );
    this.popup(
      "home-add",
      items.map(([ic, name, run]) => h("button", { type: "button", onclick: () => (this.closePopup(), run()) }, icon(ic), name)),
      h("p.home-tip", null, "Text, shapes and drawings: use the tools at the bottom."),
    );
  }

  private add(n: CNode) {
    this.view.addCard(n);
    this.openPanel();
    // A new sticky is ready to write on.
    if (n.widget === "sticky") requestAnimationFrame(() => this.board.querySelector<HTMLTextAreaElement>(`[data-id="${n.id}"] textarea`)?.focus());
  }

  private async addPicture() {
    const input = h("input", { type: "file", accept: "image/*" }) as HTMLInputElement;
    input.addEventListener("change", async () => {
      const f = input.files?.[0];
      if (!f) return;
      try {
        const path = await api.saveAttachment(new Uint8Array(await f.arrayBuffer()), f.name, "attachments");
        this.view.addCard({ id: newId(), type: "file", file: path, x: NaN, y: NaN, width: 320, height: 240, style: { look: "card" } });
      } catch (e) {
        this.app.toast(`Couldn't add the picture: ${e}`, "error");
      }
    });
    input.click();
  }

  private addLink() {
    const url = window.prompt("The web address:", "https://");
    if (!url || !/^https?:\/\/\S+/i.test(url.trim())) return;
    this.view.addCard({ id: newId(), type: "link", url: url.trim(), x: NaN, y: NaN, width: 320, height: 90, style: { look: "card" } });
  }

  private popEl: HTMLElement | null = null;
  private popup(cls: string, items: HTMLElement[], foot?: HTMLElement) {
    this.closePopup();
    const p = h(`div.home-pop.${cls}` as "div", null, ...items, foot ?? "");
    this.el.append(p);
    this.popEl = p;
    const away = (e: PointerEvent) => {
      if (p.contains(e.target as Node) || (e.target as HTMLElement).closest(".home-btn")) return;
      this.closePopup();
      document.removeEventListener("pointerdown", away, true);
    };
    document.addEventListener("pointerdown", away, true);
  }

  private closePopup() {
    this.popEl?.remove();
    this.popEl = null;
  }

  // ------------------------------------------------------------ templates

  private templateMenu() {
    this.popup("home-templates", [
      h("div.smallcaps", null, "Start from a template"),
      ...TEMPLATES.map((t) =>
        h(
          "button",
          { type: "button", title: t.about, onclick: () => (this.closePopup(), void this.useTemplate(t.board(), t.name)) },
          icon(t.icon),
          h("span", null, h("b", null, t.name), h("small", null, t.about)),
        ),
      ),
      h("hr"),
      h("button", { type: "button", onclick: () => (this.closePopup(), this.importTemplate()) }, icon("download-simple"), "Import a template…"),
      h("button", { type: "button", onclick: () => (this.closePopup(), void this.exportTemplate()) }, icon("upload-simple"), "Save this board as a template…"),
    ]);
  }

  private async useTemplate(data: CData, name: string) {
    const keep = await choose(
      `Use “${name}”?`,
      "Replace your board with it, or put it next to what's there. Either way, Undo (Ctrl+Z) brings your board back.",
      ["Replace", "Add next to it", "Cancel"],
    );
    if (keep === 2 || keep < 0) return;
    if (keep === 0) this.view.replaceAll(data);
    else {
      const now = this.view.snapshot();
      const right = Math.max(0, ...now.nodes.map((n) => n.x + n.width)) + 120;
      const left = Math.min(...data.nodes.map((n) => n.x));
      const ids = new Map<string, string>();
      for (const n of data.nodes) {
        const id = newId();
        ids.set(n.id, id);
        now.nodes.push({ ...n, id, x: n.x - left + right });
      }
      for (const e of data.edges) now.edges.push({ ...e, id: newId(), fromNode: ids.get(e.fromNode) ?? e.fromNode, toNode: ids.get(e.toNode) ?? e.toNode });
      this.view.replaceAll(now);
    }
    this.app.toast(`“${name}” is on your Home.`);
  }

  private importTemplate() {
    const input = h("input", { type: "file", accept: ".marrowhome,.json,.canvas" }) as HTMLInputElement;
    input.addEventListener("change", async () => {
      const f = input.files?.[0];
      if (!f) return;
      try {
        const t = readTemplate(await f.text());
        await this.useTemplate(t.board, t.name || displayName(f.name));
      } catch (e) {
        this.app.toast(`That file isn't a Marrow Home template: ${e instanceof Error ? e.message : e}`, "error");
      }
    });
    input.click();
  }

  private async exportTemplate() {
    const choice = await choose(
      "Save as a template",
      "A template keeps the board's cards, looks and settings. Sticky notes can keep their words, or be left empty so the template has nothing private in it.",
      ["Keep sticky words", "Leave stickies empty", "Cancel"],
    );
    if (choice === 2 || choice < 0) return;
    const name = window.prompt("Name for the template:", "My Home") ?? "";
    if (!name.trim()) return;
    const path = await saveDialog({ defaultPath: `${name.trim()}.marrowhome`, filters: [{ name: "Marrow Home template", extensions: ["marrowhome"] }] });
    if (!path) return;
    const text = writeTemplate(this.view.snapshot(), name.trim(), choice === 0);
    try {
      await api.writeExport(path, text);
      this.app.toast("Template saved. Share the file with anyone; they import it from Templates.");
    } catch (e) {
      this.app.toast(`Couldn't save the template: ${e}`, "error");
    }
  }

  // ---------------------------------------------------------- live cards

  private render(n: CNode, el: HTMLElement, changed: () => void) {
    for (const t of el.querySelectorAll<HTMLElement>("[data-timer]")) window.clearInterval(Number(t.dataset.timer));
    const cfg = (n.cfg ??= {}) as Cfg;
    const head = (title: string, ic: string, extra?: HTMLElement) => h("div.w-head", null, icon(ic), h("span.w-title", null, title), extra ?? "");
    switch (n.widget) {
      case "sticky": {
        const ta = h("textarea.w-sticky", { "data-live": "", spellcheck: true, placeholder: "Write something…" }) as HTMLTextAreaElement;
        ta.value = n.text ?? "";
        ta.addEventListener("input", () => {
          n.text = ta.value;
          changed();
        });
        el.append(ta);
        return;
      }
      case "tasks":
        return this.renderTasks(n, el, head);
      case "note":
        return this.renderNote(n, el, head);
      case "notes":
        return this.renderNotes(n, el, head);
      case "greeting":
        return this.renderGreeting(n, el);
      case "capture":
        return this.renderCapture(n, el);
      case "shortcuts":
        return this.renderShortcuts(n, el);
      case "calendar":
        return this.renderCalendar(n, el);
      case "daily":
        return this.renderDaily(el, head);
      case "countdown": {
        const date = str(cfg.date);
        const days = date ? Math.round((Date.parse(date + "T00:00:00") - Date.parse(isoDay() + "T00:00:00")) / 86400000) : NaN;
        el.append(
          h("div.w-count-title", null, str(cfg.title) || (date ? "" : "Countdown")),
          h("div.w-count", null, Number.isNaN(days) ? "—" : days === 0 ? "Today!" : String(Math.abs(days))),
          h("div.w-count-sub", null, Number.isNaN(days) ? "Pick a date in Customize" : days === 0 ? "" : days > 0 ? (days === 1 ? "day to go" : "days to go") : days === -1 ? "day ago" : "days ago"),
        );
        return;
      }
      case "clock": {
        const t = h("div.w-clock"), d = h("div.w-clock-day");
        const tick = () => {
          const now = new Date();
          t.textContent = now.toLocaleTimeString(uiLocale(), { hour: "numeric", minute: "2-digit", hour12: !cfg.h24 });
          d.textContent = now.toLocaleDateString(uiLocale(), { weekday: "long", day: "numeric", month: "long" });
        };
        tick();
        const id = window.setInterval(() => (el.isConnected ? tick() : window.clearInterval(id)), 15000);
        t.dataset.timer = String(id);
        el.append(t, d);
        return;
      }
      case "welcome": {
        const step = (n: string, t: string, d: string) => h("li.step", null, h("span.n", null, n), h("div", null, h("div.h", null, t), h("div.d", null, d)));
        el.append(
          head("Getting started", "signpost"),
          h(
            "div.w-body",
            { "data-scroll": "" },
            h(
              "ol.steps.w-steps",
              null,
              step("1", "Write", "Choose New note and start typing. Everything saves by itself."),
              step("2", "Link", "Type [[ and part of a note's name to connect two notes."),
              step("3", "Make Home yours", "Add cards with + Add, drag them anywhere, and change any card in Customize."),
            ),
            h("p.w-sub", null, "Coming from another app? Bring all your notes over, with their links, pictures and databases."),
            h(
              "div.w-shortcuts",
              null,
              h("button.btn.btn-secondary", { type: "button", onclick: () => this.app.moveFrom("notion") }, icon("notion-logo"), "From Notion"),
              h("button.btn.btn-secondary", { type: "button", onclick: () => this.app.moveFrom("evernote") }, icon("notebook"), "From Evernote"),
              h("button.btn.btn-ghost", { type: "button", onclick: () => this.app.importNotes() }, "Other apps…"),
            ),
          ),
        );
        return;
      }
      case "rediscover": {
        const box = h("div.w-body.w-rediscover", { "data-scroll": "" });
        el.append(head("Rediscover", "clock-counter-clockwise"), box);
        void renderResurface(box, this.app.resurface).then(() => {
          if (!box.childElementCount) box.append(h("p.w-empty", null, "Older notes and earlier days show up here as your vault grows."));
        }, () => box.append(h("p.w-empty", null, "Nothing to show yet.")));
        return;
      }
      default:
        el.append(h("div.w-empty", null, `This card (${n.widget}) needs a newer Marrow.`));
    }
  }

  private async allTasks(): Promise<TaskItem[]> {
    this.tasks ??= await api.tasks(false).catch(() => []);
    return this.tasks;
  }

  private renderTasks(n: CNode, el: HTMLElement, head: (t: string, i: string, x?: HTMLElement) => HTMLElement) {
    const cfg = n.cfg as Cfg;
    const list = h("div.w-body.w-tasks", { "data-scroll": "" });
    const count = h("span.w-count-badge");
    el.append(head(str(cfg.title) || "Tasks", "check-square", count), list);
    const today = isoDay();
    const filter: Partial<TaskFilter> = {
      range: str(cfg.range) || "all",
      days: num(cfg.days, 7),
      from: str(cfg.from),
      to: str(cfg.to),
      overdue: !!cfg.overdue,
      undated: !!cfg.undated,
      done: !!cfg.done,
      tag: str(cfg.tag),
      folder: str(cfg.folder),
      note: str(cfg.note),
      text: str(cfg.text),
    };
    void api.taskView(today, filter).then(
      (tasks) => {
        count.textContent = tasks.length ? String(tasks.filter((t) => !t.done).length) : "";
        if (!tasks.length) {
          list.append(h("p.w-empty", null, "Nothing to do here. 🎉"));
          return;
        }
        let group = "";
        for (const t of tasks) {
          const g = !t.due ? "No date" : t.due < today && !t.done ? "Overdue" : dayLabel(t.due, today);
          if (cfg.group !== false && g !== group) {
            group = g;
            list.append(h(`div.w-group${g === "Overdue" ? ".late" : ""}` as "div", null, g));
          }
          const box = h("button.w-check", { type: "button", "aria-label": t.done ? "Mark as not done" : "Mark as done", "aria-checked": String(t.done), role: "checkbox" }, icon(t.done ? "check-square" : "square"));
          box.addEventListener("click", async () => {
            box.closest(".w-task")?.classList.toggle("is-done");
            try {
              await toggleTaskAt(t.path, t.line);
            } catch (e) {
              this.app.toast(String(e), "error");
            }
            this.refresh();
          });
          const open = h("button.w-task-text", { type: "button", title: `Open “${displayName(t.path)}”`, onclick: () => this.app.openPath(t.path, false) }, trunc(plainText(t.text), 140));
          const meta = cfg.showNote !== false ? h("span.w-task-note", null, displayName(t.path)) : "";
          const due = cfg.group === false && t.due ? h(`span.w-task-due${t.due < today && !t.done ? ".late" : ""}` as "span", null, dayLabel(t.due, today)) : "";
          list.append(h(`div.w-task${t.done ? ".is-done" : ""}` as "div", null, box, h("div.w-task-main", null, open, h("div.w-task-meta", null, due, meta))));
        }
      },
      (e) => list.append(h("p.w-empty", null, `Couldn't read the tasks: ${e}`)),
    );
  }

  private renderNote(n: CNode, el: HTMLElement, head: (t: string, i: string, x?: HTMLElement) => HTMLElement) {
    const cfg = n.cfg as Cfg;
    const path = str(cfg.path);
    const section = str(cfg.section);
    const body = h("div.w-body.w-note.preview", { "data-scroll": "" });
    const open = h("button.w-open", { type: "button", title: "Open the note", onclick: () => path && this.app.openPath(path, false) }, icon("arrow-square-out"));
    el.append(head(path ? displayName(path) + (section ? ` › ${section}` : "") : "A note", "file-text", path ? open : undefined), body);
    if (!path) {
      body.append(h("p.w-empty", null, "Pick a note in Customize."));
      return;
    }
    void api
      .readNote(path)
      .then(async (text) => {
        const part = section ? await api.noteSection(text, section) : text;
        if (part === null) {
          body.append(h("p.w-empty", null, `No heading “${section}” in this note.`));
          return;
        }
        body.innerHTML = renderMarkdown(part.replace(/^---\n[\s\S]*?\n---\n/, ""), new Set());
        // Links open the note; everything else is just to read.
        body.addEventListener("click", (e) => {
          const a = (e.target as HTMLElement).closest("a");
          if (!a) return;
          e.preventDefault();
          const target = a.dataset.href ?? a.getAttribute("href") ?? "";
          if (/^https?:/.test(target)) return;
          this.app.openPath(path, false);
        });
      })
      .catch((e) => body.append(h("p.w-empty", null, String(e) === "LOCKED" ? "🔒 This note is locked." : "This note was moved or deleted.")));
  }

  private renderNotes(n: CNode, el: HTMLElement, head: (t: string, i: string, x?: HTMLElement) => HTMLElement) {
    const cfg = n.cfg as Cfg;
    const list = h("div.w-body.w-notes", { "data-scroll": "" });
    el.append(head(str(cfg.title) || "Notes", "list-bullets"), list);
    const limit = Math.max(1, num(cfg.limit, 8));
    const value = str(cfg.value).trim();
    const show = (paths: string[]) => {
      if (!paths.length) {
        list.append(h("p.w-empty", null, "No notes here yet."));
        return;
      }
      for (const p of paths.slice(0, limit))
        list.append(h("button.w-note-row", { type: "button", title: p, onclick: (e: MouseEvent) => this.app.openPath(p, e.ctrlKey || e.metaKey) }, icon(p.endsWith(".canvas") ? "squares-four" : "file-text"), h("span", null, displayName(p))));
    };
    const notes = () => this.app.files().filter((p) => /\.(md|canvas)$/i.test(p) && p !== HOME_FILE);
    switch (str(cfg.kind)) {
      case "starred":
        show(this.app.starred());
        break;
      case "folder": {
        const f = value.replace(/^\/+|\/+$/g, "");
        show(notes().filter((p) => (f ? p.startsWith(f + "/") : !p.includes("/"))).sort((a, b) => displayName(a).localeCompare(displayName(b))));
        break;
      }
      case "tag":
      case "search": {
        const q = str(cfg.kind) === "tag" ? `tag:#${value.replace(/^#/, "")}` : value;
        if (!value) {
          list.append(h("p.w-empty", null, str(cfg.kind) === "tag" ? "Pick a tag in Customize." : "Type a search in Customize."));
          break;
        }
        void api.search(q, limit).then((r) => show(r.hits.map((x) => x.path)), () => show([]));
        break;
      }
      default:
        void api.recentFiles(limit + 1).then((r) => show(r.map((x) => x.path).filter((p) => p !== HOME_FILE)), () => show([]));
    }
  }

  private renderGreeting(n: CNode, el: HTMLElement) {
    const cfg = n.cfg as Cfg;
    const now = new Date();
    const hr = now.getHours();
    const hello = hr < 5 ? "Good evening" : hr < 12 ? "Good morning" : hr < 18 ? "Good afternoon" : "Good evening";
    const name = str(cfg.name).trim();
    const stats = h("p.w-greet-sub");
    el.append(h("p.w-greet-hello", null, name ? `${hello}, ${name}` : hello), cfg.date !== false ? h("h1.w-greet-date", null, now.toLocaleDateString(uiLocale(), { weekday: "long", day: "numeric", month: "long" })) : "", stats);
    if (cfg.stats !== false)
      void api.vaultStats().then((s) => (stats.textContent = `${s.notes} ${s.notes === 1 ? "note" : "notes"} · ${s.links} ${s.links === 1 ? "link" : "links"}`), () => {});
  }

  private renderCapture(n: CNode, el: HTMLElement) {
    const cfg = n.cfg as Cfg;
    const toNote = str(cfg.target) === "note" && str(cfg.path);
    const input = h("input.w-capture", { type: "text", "data-live": "", placeholder: cfg.task ? "Add a task…" : toNote ? `Add to “${displayName(str(cfg.path))}”…` : "What's on your mind?" }) as HTMLInputElement;
    const more = h("button.w-capture-more", { type: "button", "data-live": "", title: "Voice, photo or more", onclick: () => this.app.capture() }, icon("microphone"));
    input.addEventListener("keydown", async (e) => {
      if (e.key !== "Enter" || !input.value.trim()) return;
      e.preventDefault();
      const words = input.value.trim().replace(/\s*\n\s*/g, " ");
      const time = new Date().toLocaleTimeString(undefined, { hour: "2-digit", minute: "2-digit" });
      const line = cfg.task ? `- [ ] ${words}` : toNote ? `- ${words}` : `- ${time} ${words}`;
      input.value = "";
      try {
        if (toNote) {
          await this.app.appendToNote(str(cfg.path), line);
          this.app.toast(`Added to “${displayName(str(cfg.path))}”.`);
        } else await this.app.appendToToday(line, cfg.task ? "Tasks" : "Log");
        this.refresh();
      } catch (err) {
        input.value = words;
        this.app.toast(String(err), "error");
      }
    });
    el.append(h("div.w-capture-row", null, icon(cfg.task ? "check-square" : "lightning"), input, more));
  }

  private renderShortcuts(n: CNode, el: HTMLElement) {
    const all: Record<string, [string, string, () => void]> = {
      new: ["note-pencil", "New note", () => this.app.newNote()],
      today: ["calendar-blank", "Today's note", () => this.app.openToday()],
      search: ["magnifying-glass", "Search", () => this.app.search()],
      ask: ["sparkle", "Ask your notes", () => this.app.ask()],
      capture: ["microphone", "Quick capture", () => this.app.capture()],
    };
    const items = Array.isArray(n.cfg?.items) ? (n.cfg!.items as string[]) : ["new", "today", "search", "ask"];
    el.append(
      h(
        "div.w-shortcuts",
        null,
        ...items.filter((k) => all[k]).map((k, i) => h(`button.btn.${i === 0 ? "btn-primary" : "btn-secondary"}` as "button", { type: "button", "data-live": "", onclick: all[k][2] }, icon(all[k][0]), all[k][1])),
      ),
    );
  }

  private renderCalendar(n: CNode, el: HTMLElement) {
    const today = isoDay();
    const shown = this.month.get(n.id) ?? today.slice(0, 7);
    const [y, m] = shown.split("-").map(Number);
    const first = new Date(y, m - 1, 1);
    const days = new Date(y, m, 0).getDate();
    const lead = (first.getDay() + 6) % 7; // Monday first
    const title = first.toLocaleDateString(uiLocale(), { month: "long", year: "numeric" });
    const go = (d: number) => {
      const t = new Date(y, m - 1 + d, 1);
      this.month.set(n.id, `${t.getFullYear()}-${String(t.getMonth() + 1).padStart(2, "0")}`);
      this.view.patched(n.id, false);
    };
    const grid = h("div.w-cal-grid");
    const names = [...Array(7)].map((_, i) => new Date(2024, 0, 1 + i).toLocaleDateString(uiLocale(), { weekday: "narrow" }));
    for (const d of names) grid.append(h("span.w-cal-wd", null, d));
    for (let i = 0; i < lead; i++) grid.append(h("span"));
    const daily = this.app.resurface.dailyIndex();
    const cells: [string, HTMLElement][] = [];
    for (let d = 1; d <= days; d++) {
      const iso = `${shown}-${String(d).padStart(2, "0")}`;
      const b = h(`button.w-cal-day${iso === today ? ".is-today" : ""}${daily.has(iso) ? ".has-note" : ""}` as "button", { type: "button", title: iso, onclick: () => this.app.openDaily(iso) }, String(d));
      cells.push([iso, b]);
      grid.append(b);
    }
    void this.allTasks().then((ts) => {
      const due = new Set(ts.map((t) => t.due).filter(Boolean));
      for (const [iso, b] of cells) b.classList.toggle("has-task", due.has(iso));
    });
    el.append(
      h(
        "div.w-cal-head",
        null,
        h("button.w-cal-nav", { type: "button", "data-live": "", title: "Previous month", onclick: () => go(-1) }, icon("caret-left")),
        h("span.w-title", null, title),
        h("button.w-cal-nav", { type: "button", "data-live": "", title: "Next month", onclick: () => go(1) }, icon("caret-right")),
      ),
      grid,
    );
  }

  private renderDaily(el: HTMLElement, head: (t: string, i: string, x?: HTMLElement) => HTMLElement) {
    const today = isoDay();
    const path = this.app.resurface.dailyIndex().get(today);
    const body = h("div.w-body.w-note.preview", { "data-scroll": "" });
    const open = h("button.w-open", { type: "button", title: "Open today's note", onclick: () => this.app.openToday() }, icon("arrow-square-out"));
    el.append(head("Today's note", "sun", open), body);
    if (!path) {
      body.append(h("p.w-empty", null, "No note for today yet."), h("button.btn.btn-secondary", { type: "button", onclick: () => this.app.openToday() }, icon("plus"), "Start today's note"));
      return;
    }
    void api.readNote(path).then(
      (text) => (body.innerHTML = renderMarkdown(text.replace(/^---\n[\s\S]*?\n---\n/, ""), new Set())),
      () => body.append(h("p.w-empty", null, "Couldn't open today's note.")),
    );
  }

  // ------------------------------------------------------------ customize

  private togglePanel() {
    if (this.panelOpen) this.closePanel();
    else this.openPanel();
  }

  private openPanel() {
    this.panelOpen = true;
    this.panel.hidden = false;
    this.syncPanel();
  }

  private closePanel() {
    this.panelOpen = false;
    this.panel.hidden = true;
  }

  private syncPanel() {
    if (!this.panelOpen) return;
    const ids = this.view.selection;
    const n = ids.length === 1 ? this.view.nodes.find((x) => x.id === ids[0]) : undefined;
    const close = h("button.home-panel-x", { type: "button", title: "Close", onclick: () => this.closePanel() }, icon("x"));
    if (!n) {
      this.panel.replaceChildren(close, ...this.boardPanel(ids.length));
      return;
    }
    this.panel.replaceChildren(close, ...this.cardPanel(n));
  }

  private boardPanel(selected: number): HTMLElement[] {
    const b = this.view.board;
    const set = (k: string, v: unknown) => {
      if (v === "" || v === undefined) delete b[k];
      else b[k] = v;
      this.saveBoard();
      this.syncPanel();
    };
    return [
      h("h3", null, "Your board"),
      h("p.home-panel-tip", null, selected > 1 ? `${selected} cards picked. Pick one card to change it.` : "Pick a card to change how it looks and what it shows."),
      field("Background", seg(["dots", "grid", "lines", "paper", "cork", "plain"], ["Dots", "Squares", "Lines", "Paper", "Corkboard", "Plain"], str(b.bg) || "dots", (v) => set("bg", v === "dots" ? "" : v))),
      field("Background colour", colorPick(str(b.color), (v) => set("color", v))),
      field(
        "Background picture",
        h(
          "div.home-row",
          null,
          h("button.btn.btn-secondary", { type: "button", onclick: () => this.pickBackground() }, icon("image"), str(b.image) ? "Change…" : "Choose…"),
          str(b.image) ? h("button.btn.btn-ghost", { type: "button", onclick: () => set("image", "") }, "Remove") : "",
        ),
      ),
      h("p.home-panel-tip", null, "Tip: drag empty space to move around; Ctrl + wheel zooms. Undo (Ctrl+Z) takes back any change."),
    ];
  }

  private saveBoard() {
    this.applyBoard();
    this.view.touch();
  }

  private pickBackground() {
    const input = h("input", { type: "file", accept: "image/*" }) as HTMLInputElement;
    input.addEventListener("change", async () => {
      const f = input.files?.[0];
      if (!f) return;
      try {
        const path = await api.saveAttachment(new Uint8Array(await f.arrayBuffer()), f.name, "attachments");
        this.view.board.image = path;
        this.saveBoard();
        this.syncPanel();
      } catch (e) {
        this.app.toast(`Couldn't add the picture: ${e}`, "error");
      }
    });
    input.click();
  }

  private cardPanel(n: CNode): HTMLElement[] {
    const def = n.widget ? WIDGETS[n.widget] : undefined;
    const s = (n.style ??= {}) as Cfg;
    const cfg = (n.cfg ??= {}) as Cfg;
    const restyle = (k: string, v: unknown) => {
      if (v === "" || v === undefined) delete s[k];
      else s[k] = v;
      this.view.patched(n.id);
      this.syncPanel();
    };
    const reconf = (k: string, v: unknown, redraw = false) => {
      cfg[k] = v;
      if (def) n.text = n.widget === "sticky" ? n.text : def.label(n);
      this.view.patched(n.id);
      if (redraw) this.syncPanel();
    };
    const title = def?.name ?? (n.type === "file" ? "Picture" : n.type === "link" ? "Link" : n.shape ? "Shape" : n.ink ? "Drawing" : "Text card");
    const out: HTMLElement[] = [h("h3", null, icon(def?.icon ?? "square"), title)];
    out.push(...this.settings(n, cfg, reconf));
    out.push(
      h("h4", null, "Look"),
      field("Style", select(LOOKS, str(s.look) || "card", (v) => restyle("look", v === "card" ? "" : v))),
      field("Colour", colorPick(str(s.bg), (v) => restyle("bg", v), true)),
      field("Colour strength", range(5, 100, num(s.tint, 18), (v) => restyle("tint", v))),
      field("Text colour", colorPick(str(s.fg), (v) => restyle("fg", v))),
      field("Font", select(FONTS, str(s.font), (v) => restyle("font", v))),
      field("Text size", seg(SIZES.map((x) => x[0]), SIZES.map((x) => x[1]), str(s.size), (v) => restyle("size", v))),
      field("Text alignment", seg(["", "center", "right"], ["Left", "Centre", "Right"], str(s.align), (v) => restyle("align", v))),
      field("Corners", range(0, 40, num(s.radius, 12), (v) => restyle("radius", v))),
      field("See-through", range(20, 100, num(s.opacity, 100), (v) => restyle("opacity", v === 100 ? "" : v))),
      h(
        "div.home-row.home-actions",
        null,
        h("button.btn.btn-secondary", { type: "button", title: "Bring to front", onclick: () => this.view.reorder(n.id, true) }, icon("stack-plus"), "Front"),
        h("button.btn.btn-secondary", { type: "button", title: "Send to back", onclick: () => this.view.reorder(n.id, false) }, icon("stack-minus"), "Back"),
        h("button.btn.btn-secondary", { type: "button", onclick: () => this.duplicate(n) }, icon("copy"), "Copy"),
        h("button.btn.btn-ghost.home-remove", { type: "button", onclick: () => this.view.removeCards([n.id]) }, icon("trash"), "Remove"),
      ),
    );
    return out;
  }

  private duplicate(n: CNode) {
    const copy = structuredClone(n) as CNode;
    copy.id = newId();
    copy.x += 30;
    copy.y += 30;
    this.view.addCard(copy);
  }

  /** The settings that are about what a card shows. */
  private settings(n: CNode, cfg: Cfg, set: (k: string, v: unknown, redraw?: boolean) => void): HTMLElement[] {
    const notes = () => this.app.files().filter((p) => p.endsWith(".md"));
    const folders = () => [...new Set(this.app.files().map((p) => p.split("/").slice(0, -1).join("/")).filter(Boolean))].sort();
    switch (n.widget) {
      case "tasks": {
        const r = str(cfg.range) || "all";
        return [
          field("Title", text(str(cfg.title), (v) => set("title", v))),
          field("Which dates", select([["days", "The next few days"], ["today", "Today"], ["range", "Between two dates"], ["all", "Any date"], ["none", "No date only"]], r, (v) => set("range", v, true))),
          r === "days" ? field("How many days", numberBox(num(cfg.days, 7), 1, 366, (v) => set("days", v))) : "",
          r === "range" ? field("From", dateBox(str(cfg.from), (v) => set("from", v))) : "",
          r === "range" ? field("To", dateBox(str(cfg.to), (v) => set("to", v))) : "",
          h("div.home-checks", null, check("Overdue too", !!cfg.overdue, (v) => set("overdue", v)), check("Tasks with no date", !!cfg.undated, (v) => set("undated", v)), check("Done tasks", !!cfg.done, (v) => set("done", v))),
          field("Only with tag", text(str(cfg.tag), (v) => set("tag", v), "#work")),
          field("Only in folder", text(str(cfg.folder), (v) => set("folder", v), "Projects", folders())),
          field("Only in note", text(str(cfg.note), (v) => set("note", v), "Path of a note", notes())),
          field("Only with words", text(str(cfg.text), (v) => set("text", v))),
          h("div.home-checks", null, check("Group by day", cfg.group !== false, (v) => set("group", v)), check("Show which note", cfg.showNote !== false, (v) => set("showNote", v))),
        ].filter(Boolean) as HTMLElement[];
      }
      case "note":
        return [field("Note", text(str(cfg.path), (v) => set("path", v), "Start typing a note's name", notes())), field("Only the part under heading", text(str(cfg.section), (v) => set("section", v), "Leave empty for the whole note"))];
      case "notes": {
        const k = str(cfg.kind) || "recent";
        return [
          field("Title", text(str(cfg.title), (v) => set("title", v))),
          field("Which notes", select([["recent", "Recently edited"], ["starred", "Starred"], ["tag", "With a tag"], ["folder", "In a folder"], ["search", "Matching a search"]], k, (v) => set("kind", v, true))),
          k === "tag" ? field("Tag", text(str(cfg.value), (v) => set("value", v), "#ideas")) : "",
          k === "folder" ? field("Folder", text(str(cfg.value), (v) => set("value", v), "Projects", folders())) : "",
          k === "search" ? field("Search for", text(str(cfg.value), (v) => set("value", v), "garden")) : "",
          field("How many", numberBox(num(cfg.limit, 8), 1, 50, (v) => set("limit", v))),
        ].filter(Boolean) as HTMLElement[];
      }
      case "greeting":
        return [field("Your name", text(str(cfg.name), (v) => set("name", v), "Optional")), h("div.home-checks", null, check("Show the date", cfg.date !== false, (v) => set("date", v)), check("Show note and link counts", cfg.stats !== false, (v) => set("stats", v)))];
      case "capture":
        return [
          field("Where it goes", select([["daily", "Today's note"], ["note", "A note I choose"]], str(cfg.target) || "daily", (v) => set("target", v, true))),
          str(cfg.target) === "note" ? field("Note", text(str(cfg.path), (v) => set("path", v), "Path of a note", notes())) : "",
          h("div.home-checks", null, check("Add as a task", !!cfg.task, (v) => set("task", v))),
        ].filter(Boolean) as HTMLElement[];
      case "shortcuts": {
        const items = Array.isArray(cfg.items) ? (cfg.items as string[]) : [];
        const opts: [string, string][] = [["new", "New note"], ["today", "Today's note"], ["search", "Search"], ["ask", "Ask your notes"], ["capture", "Quick capture"]];
        return [h("div.home-checks", null, ...opts.map(([k, l]) => check(l, items.includes(k), (v) => set("items", v ? [...items, k] : items.filter((x) => x !== k), true))))];
      }
      case "countdown":
        return [field("What for", text(str(cfg.title), (v) => set("title", v), "Holiday")), field("Date", dateBox(str(cfg.date), (v) => set("date", v)))];
      case "clock":
        return [h("div.home-checks", null, check("24-hour clock", !!cfg.h24, (v) => set("h24", v)))];
      default:
        return [];
    }
  }
}

// ------------------------------------------------------------- templates

interface Template {
  name: string;
  icon: string;
  about: string;
  board: () => CData;
}

const at = (kind: string, x: number, y: number, w?: number, hgt?: number, extra: Partial<CNode> = {}) =>
  makeCard(kind, { x, y }, { ...(w ? { width: w } : {}), ...(hgt ? { height: hgt } : {}), ...extra });

/** What a new Home starts with: what the start page had, as cards. */
export function starterBoard(): CData {
  const nodes = [
    at("greeting", 0, 0, 520, 130),
    at("shortcuts", 0, 140, 560, 64),
    at("capture", 0, 220, 520, 90),
    at("tasks", 0, 330, 520, 390),
    at("notes", 560, 0, 320, 330),
    at("daily", 560, 350, 320, 370),
    at("calendar", 920, 0, 300, 300),
    at("sticky", 920, 330, 300, 200, {
      text: "This is your Home. Drag cards to move them, pull a corner to resize, and use + Add for more. Customize changes how anything looks.",
    }),
  ];
  if (prefs.resurface) nodes.push(at("rediscover", 920, 560, 300, 240));
  return { nodes, edges: [], home: { bg: "dots" } };
}

const TEMPLATES: Template[] = [
  { name: "Starter", icon: "house", about: "Greeting, tasks for the week, today's note, recent notes and a calendar.", board: starterBoard },
  {
    name: "Week planner",
    icon: "calendar-check",
    about: "Today, the next 7 days and later on, side by side, with a few stickies.",
    board: () => ({
      nodes: [
        at("greeting", 0, 0, 940, 110, { cfg: { name: "", date: true, stats: false } }),
        at("tasks", 0, 130, 300, 460, { cfg: { ...TASKS_DEFAULT, title: "Today", range: "today", overdue: true } }),
        at("tasks", 320, 130, 300, 460, { cfg: { ...TASKS_DEFAULT, title: "This week", range: "days", days: 7, overdue: false } }),
        at("tasks", 640, 130, 300, 460, { cfg: { ...TASKS_DEFAULT, title: "Someday", range: "none", undated: true, overdue: false, group: false } }),
        at("capture", 0, 610, 620, 90, { cfg: { target: "daily", path: "", task: true } }),
        at("sticky", 640, 610, 300, 160, { text: "Top 3 this week:\n1.\n2.\n3.", style: { bg: "#ffd84d", tint: 45, look: "tape" } }),
        at("calendar", 960, 130, 300, 300),
        at("sticky", 960, 450, 300, 160, { text: "Notes to self", style: { bg: "#e64980", tint: 25, look: "pin" } }),
      ],
      edges: [],
      home: { bg: "grid" },
    }),
  },
  {
    name: "Daily life",
    icon: "sun-horizon",
    about: "A calm board for every day: clock, today's note, habits on stickies and a countdown.",
    board: () => ({
      nodes: [
        at("clock", 0, 0, 260, 120, { style: { look: "glass" } }),
        at("greeting", 280, 0, 460, 120, { cfg: { name: "", date: true, stats: false } }),
        at("daily", 0, 140, 380, 420),
        at("tasks", 400, 140, 340, 280, { cfg: { ...TASKS_DEFAULT, title: "Today", range: "today", overdue: true } }),
        at("countdown", 400, 440, 160, 120, { cfg: { title: "Next trip", date: "" }, style: { bg: "#0b8cb5", tint: 20 } }),
        at("sticky", 580, 440, 160, 120, { text: "Water 💧\nWalk 🚶\nRead 📖", style: { bg: "#2f9e44", tint: 25, look: "lined" } }),
        at("capture", 0, 580, 740, 90),
      ],
      edges: [],
      home: { bg: "paper" },
    }),
  },
  {
    name: "Study",
    icon: "graduation-cap",
    about: "Deadlines, course notes by tag, and a corkboard for reminders.",
    board: () => ({
      nodes: [
        at("tasks", 0, 0, 360, 420, { cfg: { ...TASKS_DEFAULT, title: "Deadlines (30 days)", range: "days", days: 30, overdue: true } }),
        at("notes", 380, 0, 300, 200, { cfg: { title: "Course notes", kind: "tag", value: "#course", limit: 8 } }),
        at("notes", 380, 220, 300, 200, { cfg: { title: "Recently edited", kind: "recent", value: "", limit: 6 } }),
        at("countdown", 700, 0, 240, 140, { cfg: { title: "Exams", date: "" }, style: { bg: "#e03e3e", tint: 20 } }),
        at("sticky", 700, 160, 240, 260, { text: "Formulas to remember", style: { bg: "#ffd84d", tint: 45, look: "grid" } }),
        at("capture", 0, 440, 680, 90, { cfg: { target: "daily", path: "", task: true } }),
      ],
      edges: [],
      home: { bg: "cork" },
    }),
  },
  {
    name: "Clean",
    icon: "square",
    about: "Just a greeting and a quick-note box. Add the rest yourself.",
    board: () => ({ nodes: [at("greeting", 0, 0, 520, 130), at("capture", 0, 150, 520, 90)], edges: [], home: { bg: "plain" } }),
  },
];

/** A board as a template file. Sticky notes keep their words only if `words`. */
export function writeTemplate(board: CData, name: string, words: boolean): string {
  const nodes = board.nodes.map((n) => {
    const c = structuredClone(n) as CNode;
    if (c.widget === "sticky" && !words) c.text = "";
    return c;
  });
  return JSON.stringify({ marrowHome: 1, name, made: isoDay(), board: { ...board, nodes } }, null, "\t");
}

/** A template file (or a plain canvas) read back, with fresh card ids. */
export function readTemplate(text: string): { name: string; board: CData } {
  const raw = JSON.parse(text) as Record<string, unknown>;
  const board = (raw.marrowHome ? raw.board : raw) as CData;
  if (!board || !Array.isArray(board.nodes)) throw new Error("there are no cards in it");
  const ids = new Map<string, string>();
  const nodes = board.nodes
    .filter((n) => n && typeof n === "object" && Number.isFinite(n.x) && Number.isFinite(n.y))
    .map((n) => {
      const id = newId();
      ids.set(String(n.id), id);
      return { ...n, id, width: num(n.width, 240), height: num(n.height, 160) };
    });
  const edges = (Array.isArray(board.edges) ? board.edges : [])
    .filter((e) => ids.has(String(e.fromNode)) && ids.has(String(e.toNode)))
    .map((e) => ({ ...e, id: newId(), fromNode: ids.get(String(e.fromNode))!, toNode: ids.get(String(e.toNode))! }));
  return { name: str(raw.name), board: { ...board, nodes, edges } };
}

// ------------------------------------------------------------------ bits

/** "Today", "Tomorrow", "Friday" (this week) or "3 Dec". */
export function dayLabel(day: string, today: string): string {
  const d = Math.round((Date.parse(day + "T00:00:00") - Date.parse(today + "T00:00:00")) / 86400000);
  if (d === 0) return "Today";
  if (d === 1) return "Tomorrow";
  if (d === -1) return "Yesterday";
  const date = new Date(day + "T00:00:00");
  if (d > 1 && d < 7) return date.toLocaleDateString(uiLocale(), { weekday: "long" });
  return date.toLocaleDateString(uiLocale(), { day: "numeric", month: "short", ...(day.slice(0, 4) !== today.slice(0, 4) ? { year: "numeric" } : {}) });
}

function field(label: string, control: HTMLElement): HTMLElement {
  return h("label.home-field", null, h("span.home-label", null, label), control);
}

function text(value: string, set: (v: string) => void, placeholder = "", suggestions?: string[]): HTMLElement {
  const listId = suggestions ? `home-dl-${Math.random().toString(36).slice(2)}` : undefined;
  const input = h("input.input", { type: "text", value, placeholder, ...(listId ? { list: listId } : {}) }) as HTMLInputElement;
  input.addEventListener("change", () => set(input.value.trim()));
  if (!suggestions) return input;
  return h("span.home-combo", null, input, h("datalist", { id: listId }, ...suggestions.slice(0, 400).map((s) => h("option", { value: s }))));
}

function numberBox(value: number, min: number, max: number, set: (v: number) => void): HTMLElement {
  const input = h("input.input", { type: "number", value: String(value), min: String(min), max: String(max) }) as HTMLInputElement;
  input.addEventListener("change", () => set(Math.min(max, Math.max(min, Math.round(Number(input.value) || min)))));
  return input;
}

function dateBox(value: string, set: (v: string) => void): HTMLElement {
  const input = h("input.input", { type: "date", value }) as HTMLInputElement;
  input.addEventListener("change", () => set(input.value));
  return input;
}

function check(label: string, on: boolean, set: (v: boolean) => void): HTMLElement {
  const box = h("input", { type: "checkbox", checked: on }) as HTMLInputElement;
  box.addEventListener("change", () => set(box.checked));
  return h("label.home-check", null, box, label);
}

function select(options: [string, string][], value: string, set: (v: string) => void): HTMLElement {
  const sel = h("select.input", null, ...options.map(([v, l]) => h("option", { value: v, selected: v === value }, l))) as HTMLSelectElement;
  sel.addEventListener("change", () => set(sel.value));
  return sel;
}

function seg(values: string[], labels: string[], value: string, set: (v: string) => void): HTMLElement {
  return h(
    "div.home-seg",
    null,
    ...values.map((v, i) => h(`button${v === value ? ".is-on" : ""}` as "button", { type: "button", onclick: () => set(v) }, labels[i])),
  );
}

function range(min: number, max: number, value: number, set: (v: number) => void): HTMLElement {
  const r = h("input", { type: "range", min: String(min), max: String(max), value: String(value) }) as HTMLInputElement;
  r.addEventListener("change", () => set(Number(r.value)));
  return r;
}

/** Swatches plus any colour; "" is no colour. */
function colorPick(value: string, set: (v: string) => void, sticky = false): HTMLElement {
  const custom = h("input.home-color", { type: "color", value: /^#[0-9a-f]{6}$/i.test(value) ? value : "#ffd84d", title: "Any colour" }) as HTMLInputElement;
  custom.addEventListener("change", () => set(custom.value));
  return h(
    "div.home-swatches",
    null,
    h(`button.home-swatch.is-none${value ? "" : ".is-on"}` as "button", { type: "button", title: sticky ? "No colour" : "Automatic", onclick: () => set("") }),
    ...PALETTE.map(([name, hex]) => h(`button.home-swatch${value.toLowerCase() === hex ? ".is-on" : ""}` as "button", { type: "button", title: name, style: `--sw:${hex}`, onclick: () => set(hex) })),
    custom,
  );
}

/** A question with several answers in Marrow's own dialog; the index chosen, or -1. */
function choose(title: string, lede: string, answers: string[]): Promise<number> {
  return new Promise((resolve) => {
    const close = (i: number) => {
      wrap.remove();
      document.documentElement.toggleAttribute("data-modal", false);
      document.removeEventListener("keydown", keys, true);
      resolve(i);
    };
    const keys = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        e.stopPropagation();
        close(-1);
      }
    };
    const buttons = answers.map((a, i) => h(`button.btn.${i === 0 ? "btn-primary" : i === answers.length - 1 ? "btn-ghost" : "btn-secondary"}` as "button", { type: "button", onclick: () => close(i) }, a));
    const wrap = h("div.backdrop.dlg-wrap", null, h("div.dlg", { role: "dialog", "aria-label": title }, h("h2", null, title), h("p.dlg-lede", null, lede), h("div.dlg-foot", null, ...buttons.reverse())));
    document.body.append(wrap);
    document.documentElement.toggleAttribute("data-modal", true);
    document.addEventListener("keydown", keys, true);
    buttons[buttons.length - 1].focus();
  });
}
