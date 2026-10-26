// A pane that shows one note: folder and mode switch, the title (rename by
// editing it), tags and stats, then the body as a rendered preview or as
// Markdown source in CodeMirror. The main area has one; the split has another.

import { convertFileSrc } from "@tauri-apps/api/core";
import { openUrl } from "@tauri-apps/plugin-opener";
import { foldable, foldedRanges, foldEffect } from "@codemirror/language";
import type { EditorState } from "@codemirror/state";
import { EditorView } from "@codemirror/view";

import { api, displayName, parentOf, type NoteInfo } from "./api";
import { isLockedNote, lockedFolderOf } from "./lock";
import { ATTACHMENT_FOLDER, draggingFiles, filesIn, pasteClipboardImage, saveFiles } from "./attach";
import { CALLOUTS } from "./callouts";
import { anyDocChanged, Doc, edit, findDoc, replaceText, type Origin } from "./docs";
import { embedRange, ensureBlockId, rangeText, replaceRange } from "./embeds";
import { applyText, createEditor, makeState, revealPos, updateUnresolved, wikilinkAt, type EditorHooks } from "./editor";
import { hideCard, hoverCard, leaveCard, type CardContent } from "./hover";
import type { Mode } from "./prefs";
import { props, setProp } from "./frontmatter";
import { setLivePreview } from "./livepreview";
import { applyWriting } from "./writing";
import { showMenu } from "./menu";
import { prefs } from "./prefs";
import { BlockHandles } from "./blocks";
import { highlightCodeBlocks, renderMermaid } from "./codeblocks";
import { openDiagramEditor } from "./diagram-editor";
import { FormatBar } from "./format";
import { Database, editNote, setConfigKey } from "./database";
import { linkTarget, renderMarkdown, type ButtonSpec } from "./preview";
import { PropertyHeader } from "./properties";
import { TableEditor } from "./tables";
import { ago, h, headings, icon, kbd, storage, store, wordCount } from "./util";

export interface At {
  line: number;
  ch: number;
  len?: number;
}

export interface DailyNav {
  label: string;
  prev: string | null;
  next: string | null;
}

export interface PaneApp {
  vaultRoot(): string;
  /** A wikilink or Markdown link was followed. */
  openLink(target: string, from: NotePane, inSplit: boolean): void;
  openPath(path: string, from: NotePane, inSplit: boolean): void;
  openTag(tag: string): void;
  rename(path: string, newName: string): Promise<boolean>;
  dailyNav(path: string): DailyNav | null;
  /** The pane's text changed (typing, or from elsewhere). */
  onEdited(pane: NotePane): void;
  onModeChanged(pane: NotePane): void;
  onCursor(pane: NotePane): void;
  /** Tags, backlink count or links were re-read. */
  onInfo(pane: NotePane): void;
  closeSplit(): void;
  /** Notes in the templates folder. */
  templates(): { name: string; path: string }[];
  /** A template's text, with {{title}}, {{date}} and {{time}} filled in. */
  templateText(path: string, title: string): Promise<string>;
  /** Buttons that create or open other notes. */
  runButton(spec: ButtonSpec, from: NotePane): void;
  /** Lets the person pick an image; returns its vault path. */
  pickImage(): Promise<string | null>;
  /** Every note's name, for link suggestions. */
  noteNames(): string[];
  toast(text: string, kind?: "info" | "error"): void;
}

const EMOJI = (
  "📝 📌 📎 📚 📖 🗂️ 🗃️ 🗒️ 📅 🗓️ ⏰ ✅ ☑️ ⭐ 🌟 ✨ 💡 🔥 🚀 🎯 🧭 🧠 💭 🔍 " +
  "🏠 🏢 🎓 🧪 🧬 🔬 🛠️ ⚙️ 💻 📱 🖋️ ✏️ 🎨 🎵 📷 🎬 🌱 🌿 🌳 🌊 ⛰️ ☀️ 🌙 ❄️ " +
  "❤️ 💙 💚 💛 🧡 💜 🖤 🤍 🍎 ☕ 🍵 🧳 ✈️ 🚲 🏃 💰 📈 🗺️ 🏷️ 🔖 🧩 🎁 🔒 📣"
).split(" ");

/** Fold and collapse state per note, keyed by the heading line's text. */
const foldsKey = (root: string) => `marrow.folds.${root}`;

export class NotePane {
  view: EditorView;
  doc: Doc | null = null;
  mode: Mode = "preview";
  info: NoteInfo | null = null;
  unresolved = new Set<string>();

  private scroller: HTMLElement;
  private crumbPath: HTMLElement;
  private daynav: HTMLElement;
  private segPreview: HTMLButtonElement;
  private segSource: HTMLButtonElement;
  private title: HTMLElement;
  private cover: HTMLElement;
  private iconEl: HTMLElement;
  private addBar: HTMLElement;
  private decor = { icon: "", cover: "" };
  private meta: HTMLElement;
  private propsHeader: PropertyHeader;
  private formatBar: FormatBar;
  private blocks: BlockHandles;
  private tables: TableEditor;
  private preview: HTMLElement;
  private source: HTMLElement;
  private applying = false;
  private previewTimer: number | undefined;
  private renderedFor: string | null = null;
  private refreshSeq = 0;
  private embedTimer: number | undefined;
  /** Where the reader was when an edit made from Preview arrived; put back after the redraw. */
  private keepTop: number | null = null;
  /** The hidden source editor is behind the doc (updated when it's shown). */
  private editorStale = false;
  private readonly listener = (origin: Origin) => this.onDocChanged(origin);

  readonly hooks: EditorHooks;

  constructor(
    readonly root: HTMLElement,
    readonly kind: "primary" | "split",
    private app: PaneApp,
  ) {
    this.hooks = {
      onChange: () => {
        if (!this.doc || this.applying) return;
        edit(this.doc, this.view.state.doc.toString(), this);
        this.app.onEdited(this);
      },
      onFollowLink: (target, inSplit) => this.app.openLink(target, this, !!inSplit),
      onTag: (tag) => this.app.openTag(tag),
      onCursor: () => this.app.onCursor(this),
      currentPath: () => this.doc?.path ?? null,
      slash: {
        templates: async () => this.app.templates(),
        templateText: (path) => this.app.templateText(path, this.doc ? displayName(this.doc.path) : ""),
        editDiagram: (line) => this.editDiagramAt(line),
      },
      onFolds: () => this.saveFolds(),
      onSelection: (state) => this.formatBar?.sync(state),
    };

    if (kind === "split") {
      root.append(
        h(
          "div.pane-split-head",
          null,
          icon("columns"),
          h("span", null, "Split pane"),
          h("button", { title: "Close split", onclick: () => this.app.closeSplit() }, icon("x")),
        ),
      );
    }

    this.crumbPath = h("span.path");
    this.daynav = h("span.daynav");
    this.segPreview = h("button", { onclick: () => this.setMode("preview") }, icon("book-open"), "Preview");
    this.segSource = h("button", { onclick: () => this.setMode("source") }, icon("code"), "Source");
    this.title = h("div.note-title", {
      contenteditable: "plaintext-only",
      spellcheck: false,
      role: "textbox",
      "aria-label": "Note title (edit to rename)",
      "data-placeholder": "Untitled",
    });
    this.meta = h("div.note-meta");
    this.cover = h("div.note-cover", { hidden: true });
    this.iconEl = h("button.note-icon", { hidden: true, title: "Change icon", onclick: () => this.pickIcon(this.iconEl) });
    this.addBar = h(
      "div.note-add",
      null,
      h("button.add-icon", { onclick: (e: MouseEvent) => this.pickIcon(e.currentTarget as HTMLElement) }, icon("smiley"), "Add icon"),
      h("button.add-cover", { onclick: () => void this.pickCover() }, icon("image"), "Add cover"),
      h(
        "button.add-prop",
        {
          onclick: () => {
            if (this.mode === "source") this.setMode("preview");
            this.propsHeader.add();
          },
        },
        icon("list-plus"),
        "Add property",
      ),
    );
    this.propsHeader = new PropertyHeader({
      doc: () => this.doc,
      openLink: (target) => this.app.openLink(target, this, false),
      openTag: (tag) => this.app.openTag(tag),
      noteNames: () => this.app.noteNames(),
    });
    this.preview = h("div.preview");
    this.source = h("div.source");
    this.formatBar = new FormatBar(() => this.view);
    const col = h("div.pane-col");
    this.scroller = h(
      "div.pane-scroll",
      { tabindex: "-1" },
      col,
    );
    col.append(
        this.cover,
        h(
          "div.note-head",
          null,
          h("div.crumbs", null, icon("folder-simple"), this.crumbPath, this.daynav),
          h("div.seg.small", { role: "group", "aria-label": "View" }, this.segPreview, this.segSource),
        ),
        this.iconEl,
        this.addBar,
        this.title,
        this.meta,
        this.propsHeader.el,
        this.preview,
        this.formatBar.el,
        this.source,
    );
    root.append(this.scroller);
    this.blocks = new BlockHandles({
      doc: () => this.doc,
      preview: this.preview,
      col,
      menuExtras: (u) => [
        {
          label: "Copy as synced block",
          icon: "arrows-clockwise",
          run: () => void this.copySynced(u.el, u.start, u.end, u.li),
        },
      ],
    });
    // Embeds of other notes redraw when those notes change.
    const redrawEmbeds = (path?: string) => {
      if (this.mode !== "preview" || !this.doc || this.preview.querySelector(".embed.is-editing")) return;
      const embeds = this.preview.querySelectorAll<HTMLElement>(".embed[data-path]");
      if (![...embeds].some((e) => !path || e.dataset.path === path)) return;
      window.clearTimeout(this.embedTimer);
      this.embedTimer = window.setTimeout(() => {
        this.renderedFor = null;
        this.renderPreview();
      }, 300);
    };
    anyDocChanged.add((d) => d !== this.doc && redrawEmbeds(d.path));
    window.addEventListener("marrow-vault-changed", () => redrawEmbeds());
    this.tables = new TableEditor({
      doc: () => this.doc,
      preview: this.preview,
      rerender: () => {
        if (this.keepTop === null) this.keepTop = this.scroller.scrollTop;
        this.renderedFor = null;
        this.renderPreview();
      },
    });
    this.view = createEditor(this.source, this.hooks);
    this.bindTitle();
    this.bindPreview();
    this.bindFiles();
    this.bindHover();
  }

  // -------------------------------------------------------- link previews

  /** Hover a link in Preview, or Ctrl+hover one in Source, to see the note. */
  private bindHover() {
    this.preview.addEventListener("mouseover", (e) => {
      const a = (e.target as HTMLElement).closest<HTMLElement>("a[data-link]");
      if (!a || !this.preview.contains(a)) return;
      const link = a.dataset.link!;
      hoverCard(`${this.kind}:${link}`, a.getBoundingClientRect(), () => this.linkCard(link));
    });
    this.preview.addEventListener("mouseout", (e) => {
      const a = (e.target as HTMLElement).closest("a[data-link]");
      if (a && !a.contains(e.relatedTarget as Node)) leaveCard();
    });
    const dom = this.view.dom;
    dom.addEventListener("mousemove", (e) => {
      if (!(e.ctrlKey || e.metaKey)) return leaveCard();
      const pos = this.view.posAtCoords({ x: e.clientX, y: e.clientY });
      const link = pos == null ? null : wikilinkAt(this.view.state, pos);
      if (!link) return leaveCard();
      const c = this.view.coordsAtPos(pos!);
      const rect = c ? new DOMRect(c.left, c.top, 1, c.bottom - c.top) : new DOMRect(e.clientX, e.clientY, 1, 16);
      hoverCard(`${this.kind}:src:${link}`, rect, () => this.linkCard(link.split("|")[0]), 200);
    });
    dom.addEventListener("mouseleave", () => leaveCard());
  }

  // ---------------------------------------------------------- exporting

  /**
   * The note as it reads in Preview, ready to print or save: title, rendered
   * content, diagrams and images, without the editing buttons. Collapsed
   * sections are opened up.
   */
  async exportContent(): Promise<{ title: string; el: HTMLElement } | null> {
    const doc = this.doc;
    if (!doc) return null;
    const title = displayName(doc.path);
    let el: HTMLElement;
    if (this.mode === "preview" && this.renderedFor === doc.text) {
      el = this.preview.cloneNode(true) as HTMLElement;
      // Databases draw themselves; a clone of their DOM is what we want.
    } else {
      el = h("div");
      el.innerHTML = renderMarkdown(doc.text, this.unresolved);
      await highlightCodeBlocks(el);
      await renderMermaid(el);
      await this.hydrate(el, doc.path);
    }
    el.removeAttribute("id");
    el.removeAttribute("hidden");
    el.className = "preview";
    el.querySelectorAll("button, .h-fold, .li-fold, .tbl-add, .block-drop, .block-handle, .preview-empty, input[type=file]").forEach((x) => x.remove());
    el.querySelectorAll(".is-hidden").forEach((x) => x.classList.remove("is-hidden"));
    el.querySelectorAll(".is-collapsed").forEach((x) => x.classList.remove("is-collapsed"));
    el.querySelectorAll("details").forEach((d) => d.setAttribute("open", ""));
    el.querySelectorAll("[contenteditable]").forEach((x) => x.removeAttribute("contenteditable"));
    // The title, unless the note already starts with the same heading.
    const first = el.firstElementChild;
    const same = first?.tagName === "H1" && first.textContent?.trim().toLowerCase() === title.toLowerCase();
    if (!same) el.prepend(h("h1.export-title", null, title));
    return { title, el };
  }

  /** What a link's hover card shows: the note (or its section), or the image. */
  private async linkCard(link: string): Promise<CardContent | null> {
    const doc = this.doc;
    if (!doc) return null;
    const target = linkTarget(link);
    const sub = link.includes("#") ? link.slice(link.indexOf("#") + 1) : "";
    const path = target ? await api.resolveLink(target, doc.path).catch(() => null) : doc.path;
    if (!path) {
      return { title: target, icon: "file-dashed", body: h("p.hover-missing", null, "This note doesn't exist yet. Click the link to create it.") };
    }
    const open = () => {
      if (!target) this.jumpToHeading(sub);
      else this.app.openLink(target, this, false);
    };
    if (!path.toLowerCase().endsWith(".md")) {
      if (!/\.(png|jpe?g|gif|webp|svg|avif|bmp)$/i.test(path)) return null;
      return { title: displayName(path), icon: "image", body: h("img.hover-img", { src: this.assetUrl(path), alt: displayName(path) }), open };
    }
    const text = findDoc(path)?.text ?? (await api.readNote(path).catch(() => null));
    if (text === null) return null;
    const range = sub ? embedRange(text, sub) : null;
    let part = range ? rangeText(text, range) : text;
    // Long notes: the beginning is enough for a preview.
    const lines = part.split("\n");
    if (lines.length > 160) part = lines.slice(0, 160).join("\n") + "\n\n…";
    const body = h("div");
    if (!part.trim()) body.append(h("p.hover-missing", null, "This note is empty."));
    else {
      body.innerHTML = renderMarkdown(part, new Set(), 1);
      for (const el of body.querySelectorAll("[data-line]")) el.removeAttribute("data-line");
      for (const el of body.querySelectorAll("[data-task]")) el.removeAttribute("data-task");
      void this.hydrate(body, path);
      body.addEventListener("click", (e) => {
        const a = (e.target as HTMLElement).closest<HTMLElement>("a[data-link], a[data-href]");
        if (!a) return;
        e.preventDefault();
        hideCard();
        if (a.dataset.href) void openUrl(a.dataset.href).catch(() => {});
        else if (linkTarget(a.dataset.link!)) this.app.openLink(linkTarget(a.dataset.link!), this, false);
      });
    }
    return { title: displayName(path) + (sub && range ? ` › ${sub.replace(/^\^/, "")}` : ""), icon: "file-text", body, open };
  }

  // ------------------------------------------------------- pasting files

  /** Pasted or dropped images and files are saved to the vault and linked. */
  private bindFiles() {
    const dom = this.view.dom;
    dom.addEventListener(
      "paste",
      (e) => {
        const files = filesIn(e.clipboardData);
        if (!this.doc) return;
        const types = Array.from(e.clipboardData?.types ?? []);
        if (!files.length) {
          // Nothing the page can read: maybe an image only the system clipboard has.
          if (types.some((t) => t.startsWith("text/"))) return;
          e.preventDefault();
          const doc = this.doc;
          const sel = this.view.state.selection.main;
          void pasteClipboardImage(doc.path).then(
            (md) => {
              if (!md || this.doc !== doc || this.mode !== "source") return;
              this.view.dispatch({ changes: { from: sel.from, to: sel.to, insert: md }, selection: { anchor: sel.from + md.length }, userEvent: "input.paste", scrollIntoView: true });
              this.app.toast(`Added the image to “${ATTACHMENT_FOLDER}”.`);
            },
            (err) => this.app.toast(`Couldn't paste the image: ${err}`, "error"),
          );
          return;
        }
        e.preventDefault();
        e.stopPropagation();
        const r = this.view.state.selection.main;
        void this.insertFiles(files, true, (md) => ({ from: r.from, to: r.to, md }));
      },
      true,
    );
    dom.addEventListener("dragover", (e) => {
      if (!draggingFiles(e)) return;
      e.preventDefault();
      e.dataTransfer!.dropEffect = "copy";
    });
    dom.addEventListener(
      "drop",
      (e) => {
        const files = filesIn(e.dataTransfer);
        if (!files.length || !this.doc) return;
        e.preventDefault();
        e.stopPropagation();
        const pos = this.view.posAtCoords({ x: e.clientX, y: e.clientY }) ?? this.view.state.selection.main.head;
        void this.insertFiles(files, false, (md) => ({ from: pos, to: pos, md }));
      },
      true,
    );

    // Preview: the file goes in after the block it was dropped on.
    const col = this.preview;
    let over = 0;
    const clear = () => {
      over = 0;
      col.classList.remove("is-file-drop");
    };
    col.addEventListener("dragenter", (e) => {
      if (!draggingFiles(e)) return;
      over++;
      col.classList.add("is-file-drop");
    });
    col.addEventListener("dragleave", () => {
      if (--over <= 0) clear();
    });
    col.addEventListener("dragover", (e) => {
      if (!draggingFiles(e)) return;
      e.preventDefault();
      e.dataTransfer!.dropEffect = "copy";
    });
    col.addEventListener("drop", (e) => {
      clear();
      const files = filesIn(e.dataTransfer);
      if (!files.length || !this.doc) return;
      e.preventDefault();
      const line = this.lineAfterBlockAt(e.clientY);
      void this.insertFiles(files, false, (md) => {
        const text = this.doc!.text;
        const lines = text.split("\n");
        if (line >= lines.length) return { from: text.length, to: text.length, md: `${text.trim() ? "\n\n" : ""}${md}\n`, whole: true };
        const at = lines.slice(0, line).join("\n").length;
        return { from: at, to: at, md: `\n\n${md}${(lines[line] ?? "").trim() ? "\n" : ""}`, whole: true };
      });
    });
  }

  /** The line just after the top-level block at height `y` (or the end of the note). */
  private lineAfterBlockAt(y: number): number {
    const lines = (this.doc?.text ?? "").split("\n");
    const blocks = (Array.from(this.preview.children) as HTMLElement[]).filter((c) => c.dataset.line !== undefined);
    let pick = -1;
    for (let i = 0; i < blocks.length; i++) if (blocks[i].getBoundingClientRect().top <= y) pick = i;
    if (pick < 0) return blocks.length ? Number(blocks[0].dataset.line) : lines.length;
    let end = pick + 1 < blocks.length ? Number(blocks[pick + 1].dataset.line) : lines.length;
    while (end > Number(blocks[pick].dataset.line) + 1 && (lines[end - 1] ?? "").trim() === "") end--;
    return end;
  }

  private async insertFiles(files: File[], pasted: boolean, where: (md: string) => { from: number; to: number; md: string; whole?: boolean }) {
    const doc = this.doc;
    if (!doc) return;
    let md: string;
    try {
      md = await saveFiles(files, doc.path, pasted);
    } catch (e) {
      this.app.toast(`Couldn't add the file: ${e}`, "error");
      return;
    }
    if (this.doc !== doc) return;
    const w = where(md);
    if (w.whole || this.mode !== "source") {
      const t = doc.text;
      if (this.keepTop === null) this.keepTop = this.scroller.scrollTop;
      replaceText(doc, t.slice(0, w.from) + w.md + t.slice(w.to));
    } else {
      this.view.dispatch({ changes: { from: w.from, to: w.to, insert: w.md }, selection: { anchor: w.from + w.md.length }, userEvent: "input.paste", scrollIntoView: true });
      this.view.focus();
    }
    this.app.toast(files.length === 1 ? `Added ${files[0].name || "the image"} to “${ATTACHMENT_FOLDER}”.` : `Added ${files.length} files to “${ATTACHMENT_FOLDER}”.`);
  }

  // ---------------------------------------------------------------- showing

  /** Shows `doc`. `state` is a tab's kept editor state (undo, cursor). */
  show(
    doc: Doc,
    opts: { state?: EditorState | null; mode: Mode; scrollTop?: number; at?: At; focusTitle?: boolean },
  ) {
    if (this.doc !== doc) {
      this.doc?.listeners.delete(this.listener);
      doc.listeners.add(this.listener);
    }
    this.doc = doc;
    this.editorStale = false;
    this.keepTop = null;
    this.info = null;
    this.unresolved = new Set();
    this.renderedFor = null;
    const state = opts.state ?? makeState(doc.text, this.hooks);
    this.view.setState(state);
    setLivePreview(this.view, prefs.livePreview); // a kept state may predate the setting
    applyWriting(this.view);
    if (state.doc.toString() !== doc.text) this.applyQuietly(doc.text);
    if (!opts.state) this.restoreFolds();
    this.mode = opts.mode;
    this.renderDecor(true);
    this.propsHeader.render(true);
    this.formatBar.sync(this.view.state);
    this.renderHead();
    this.renderMeta();
    this.applyMode();
    this.root.hidden = false;
    const top = opts.scrollTop ?? 0;
    requestAnimationFrame(() => {
      this.scroller.scrollTop = top;
      if (opts.at) this.reveal(opts.at);
    });
    if (opts.focusTitle) this.focusTitle();
    else if (!opts.at) this.focus();
    void this.refresh();
  }

  /** Brings the hidden editor up to date with the doc. */
  private syncEditor() {
    if (!this.editorStale || !this.doc) return;
    this.editorStale = false;
    if (this.view.state.doc.toString() !== this.doc.text) this.applyQuietly(this.doc.text);
  }

  /** What a tab keeps when it goes to the background. */
  stash(): { state: EditorState; scrollTop: number } {
    this.syncEditor();
    return { state: this.view.state, scrollTop: this.scroller.scrollTop };
  }

  clear() {
    this.doc?.listeners.delete(this.listener);
    this.doc = null;
    this.info = null;
    this.view.setState(makeState("", this.hooks));
    this.preview.replaceChildren();
    this.renderedFor = null;
  }

  get path(): string | null {
    return this.doc?.path ?? null;
  }

  get scrollTop() {
    return this.scroller.scrollTop;
  }

  focus() {
    if (this.mode === "source") this.view.focus();
    else this.scroller.focus({ preventScroll: true });
  }

  focusTitle() {
    this.title.focus();
    const r = document.createRange();
    r.selectNodeContents(this.title);
    const sel = window.getSelection();
    sel?.removeAllRanges();
    sel?.addRange(r);
  }

  /** The file was renamed or moved. */
  pathChanged() {
    this.renderHead();
  }

  /** Links, tags and backlinks may have changed. */
  async refresh() {
    const doc = this.doc;
    if (!doc) return;
    const mine = ++this.refreshSeq;
    const [info, links] = await Promise.all([
      api.noteInfo(doc.path).catch(() => null),
      api.noteLinks(doc.path).catch(() => ({ outgoing: [], unresolved: [] as string[] })),
    ]);
    if (mine !== this.refreshSeq || this.doc !== doc) return;
    this.info = info;
    const un = new Set(links.unresolved.map((n) => n.replace(/\.md$/i, "").toLowerCase()));
    const changed = un.size !== this.unresolved.size || [...un].some((n) => !this.unresolved.has(n));
    this.unresolved = un;
    updateUnresolved(this.view, links.unresolved);
    this.renderMeta();
    this.app.onInfo(this);
    if (changed) this.renderedFor = null;
    if (this.mode === "preview") this.renderPreview();
  }

  // ------------------------------------------------------------------ modes

  setMode(mode: Mode, line?: number) {
    if (!this.doc) return;
    const at = line ?? this.topLine();
    if (mode === this.mode) return;
    this.mode = mode;
    this.applyMode();
    requestAnimationFrame(() => {
      this.scrollToLine(at);
      if (mode === "source" && line !== undefined) {
        const l = this.view.state.doc.line(Math.min(line + 1, this.view.state.doc.lines));
        this.view.dispatch({ selection: { anchor: l.from } });
      }
      this.focus();
    });
    this.app.onModeChanged(this);
  }

  toggleMode() {
    this.setMode(this.mode === "preview" ? "source" : "preview");
  }

  private applyMode() {
    const pv = this.mode === "preview";
    this.root.dataset.mode = this.mode;
    if (!pv) this.blocks.hide();
    this.segPreview.setAttribute("aria-pressed", String(pv));
    this.segSource.setAttribute("aria-pressed", String(!pv));
    this.preview.hidden = !pv;
    this.source.hidden = pv;
    if (!pv) this.syncEditor();
    if (pv) this.renderPreview();
    else this.view.requestMeasure();
  }

  /** The note line at the top of the visible area. */
  topLine(): number {
    const top = this.scroller.getBoundingClientRect().top + 8;
    if (this.mode === "preview") {
      let best = 0;
      for (const el of this.preview.querySelectorAll<HTMLElement>("[data-line]")) {
        if (el.closest(".embed")) continue;
        if (el.getBoundingClientRect().top > top) break;
        best = Number(el.dataset.line);
      }
      return best;
    }
    const editorTop = this.view.dom.getBoundingClientRect().top;
    if (editorTop >= top) return 0;
    const block = this.view.lineBlockAtHeight(top - editorTop);
    return this.view.state.doc.lineAt(block.from).number - 1;
  }

  scrollToLine(line: number, flash = false) {
    if (line <= 0) {
      this.scroller.scrollTop = 0;
      return;
    }
    const scTop = this.scroller.getBoundingClientRect().top;
    if (this.mode === "preview") {
      let target: HTMLElement | null = null;
      for (const el of this.preview.querySelectorAll<HTMLElement>("[data-line]")) {
        if (el.closest(".embed")) continue;
        if (Number(el.dataset.line) > line) break;
        target = el;
      }
      if (!target) return;
      this.scroller.scrollTop += target.getBoundingClientRect().top - scTop - 30;
      if (flash) {
        target.classList.remove("flash");
        void target.offsetWidth;
        target.classList.add("flash");
      }
    } else {
      const doc = this.view.state.doc;
      const pos = doc.line(Math.min(line + 1, doc.lines)).from;
      const editorTop = this.view.dom.getBoundingClientRect().top;
      this.scroller.scrollTop += editorTop + this.view.lineBlockAt(pos).top - scTop - 30;
    }
  }

  /** Shows a place in the note: selects it in source, highlights it in preview. */
  reveal(at: At) {
    if (this.mode === "source") revealPos(this.view, at.line, at.ch, at.len ?? 0);
    else this.scrollToLine(at.line, true);
  }

  // ------------------------------------------------------------ doc changes

  private applyQuietly(text: string) {
    this.applying = true;
    try {
      applyText(this.view, text);
    } finally {
      this.applying = false;
    }
  }

  private onDocChanged(origin: Origin) {
    const doc = this.doc;
    if (!doc) return;
    // A hidden editor that takes a change can scroll the pane, so it catches up when shown.
    if (origin !== this && this.mode === "preview") {
      this.editorStale = true;
      // Clicking elsewhere to finish an edit can make the browser scroll; the redraw undoes that.
      if (this.keepTop === null) this.keepTop = this.scroller.scrollTop;
    }
    else if (origin !== this && this.view.state.doc.toString() !== doc.text) this.applyQuietly(doc.text);
    this.renderMeta();
    this.renderDecor();
    this.propsHeader.render();
    if (this.mode === "preview") {
      window.clearTimeout(this.previewTimer);
      this.previewTimer = window.setTimeout(() => this.renderPreview(), origin === "app" ? 0 : 150);
    }
    if (origin !== this) this.app.onEdited(this);
  }

  // ------------------------------------------------------- icon and cover

  /** Shows `icon:` and `cover:` from the frontmatter. */
  private renderDecor(force = false) {
    const doc = this.doc;
    if (!doc) return;
    const p = props(doc.text);
    const iconv = typeof p.icon === "string" ? p.icon.trim() : "";
    const coverv = typeof p.cover === "string" ? p.cover.trim().replace(/^\[\[(.*)\]\]$/, "$1") : "";
    if (!force && iconv === this.decor.icon && coverv === this.decor.cover) return;
    this.decor = { icon: iconv, cover: coverv };
    this.iconEl.hidden = !iconv;
    this.iconEl.textContent = iconv;
    (this.addBar.querySelector(".add-icon") as HTMLElement).hidden = !!iconv;
    (this.addBar.querySelector(".add-cover") as HTMLElement).hidden = !!coverv;
    this.cover.hidden = !coverv;
    this.root.classList.toggle("has-cover", !!coverv);
    if (!coverv) {
      this.cover.replaceChildren();
      return;
    }
    const img = h("img", { alt: "" }) as HTMLImageElement;
    const bar = h(
      "div.cover-tools",
      null,
      h("button", { onclick: () => void this.pickCover() }, "Change cover"),
      h("button", { onclick: () => this.setDecor("cover", undefined) }, "Remove"),
    );
    this.cover.replaceChildren(img, bar);
    if (/^https?:\/\//i.test(coverv)) img.src = coverv;
    else {
      const path = doc.path;
      void api.resolveLink(coverv, path).then((found) => {
        if (this.doc?.path !== path) return;
        img.src = this.assetUrl(found ?? coverv);
      });
    }
  }

  private setDecor(key: "icon" | "cover", value: string | undefined) {
    const doc = this.doc;
    if (!doc) return;
    const next = setProp(doc.text, key, value);
    if (next !== doc.text) replaceText(doc, next);
  }

  private pickIcon(anchor: HTMLElement) {
    document.querySelector(".icon-pop")?.remove();
    const close = () => {
      pop.remove();
      document.removeEventListener("mousedown", outside, true);
    };
    const outside = (e: MouseEvent) => {
      if (!pop.contains(e.target as Node)) close();
    };
    const choose = (v: string | undefined) => {
      close();
      this.setDecor("icon", v);
    };
    const input = h("input.input", { placeholder: "Or type any emoji or symbol", maxlength: 8 }) as HTMLInputElement;
    input.addEventListener("keydown", (e) => {
      if (e.key === "Enter" && input.value.trim()) choose(input.value.trim());
      if (e.key === "Escape") close();
    });
    const pop = h(
      "div.icon-pop",
      null,
      h("div.icon-grid", null, ...EMOJI.map((em) => h("button", { onclick: () => choose(em) }, em))),
      h("div.icon-foot", null, input, this.decor.icon ? h("button.btn.btn-secondary", { onclick: () => choose(undefined) }, "Remove") : ""),
    );
    document.body.append(pop);
    const r = anchor.getBoundingClientRect();
    pop.style.left = `${Math.min(r.left, innerWidth - pop.offsetWidth - 12)}px`;
    pop.style.top = `${Math.min(r.bottom + 6, innerHeight - pop.offsetHeight - 12)}px`;
    document.addEventListener("mousedown", outside, true);
  }

  private async pickCover() {
    const path = await this.app.pickImage();
    if (path) this.setDecor("cover", path);
  }

  // ------------------------------------------------------------ folding

  private foldStore(): Record<string, string[]> {
    return storage<Record<string, string[]>>(foldsKey(this.app.vaultRoot()), {});
  }

  /** Lines (by their text) folded in this note. */
  private foldedLines(): Set<string> {
    return new Set(this.doc ? this.foldStore()[this.doc.path] ?? [] : []);
  }

  private writeFolds(lines: Set<string>) {
    if (!this.doc) return;
    const all = this.foldStore();
    if (lines.size) all[this.doc.path] = [...lines];
    else delete all[this.doc.path];
    store(foldsKey(this.app.vaultRoot()), all);
  }

  /** Source mode folded or unfolded something. */
  private saveFolds() {
    const st = this.view.state;
    const lines = new Set<string>();
    foldedRanges(st).between(0, st.doc.length, (from) => {
      lines.add(st.doc.lineAt(from).text.trim());
    });
    this.writeFolds(lines);
  }

  /** Folds what was folded last time. */
  private restoreFolds() {
    const want = this.foldedLines();
    if (!want.size) return;
    const st = this.view.state;
    const effects = [];
    for (let n = 1; n <= st.doc.lines; n++) {
      const line = st.doc.line(n);
      if (!want.has(line.text.trim())) continue;
      const range = foldable(st, line.from, line.to);
      if (range) effects.push(foldEffect.of(range));
    }
    if (effects.length) this.view.dispatch({ effects });
  }

  /** Carets on headings and nested lists in Preview, honouring saved state. */
  private decoratePreview() {
    const doc = this.doc;
    if (!doc) return;
    const lines = doc.text.split("\n");
    const folded = this.foldedLines();
    for (const el of this.preview.querySelectorAll<HTMLElement>(":scope > h1, :scope > h2, :scope > h3, :scope > h4, :scope > h5, :scope > h6")) {
      const key = (lines[Number(el.dataset.line)] ?? "").trim();
      el.dataset.key = key;
      el.classList.toggle("is-collapsed", folded.has(key));
      el.prepend(h("span.h-fold", { title: folded.has(key) ? "Expand section" : "Collapse section", "aria-hidden": "true" }, icon("caret-down")));
    }
    for (const li of this.preview.querySelectorAll<HTMLElement>("li")) {
      if (li.closest(".embed") || !li.querySelector(":scope > ul, :scope > ol, :scope > .body > ul, :scope > .body > ol")) continue;
      li.classList.add("has-kids");
      li.prepend(h("span.li-fold", { title: "Collapse", "aria-hidden": "true" }, icon("caret-down")));
    }
    this.applyCollapse();
  }

  /** Hides everything under a collapsed heading, up to the next heading as big. */
  private applyCollapse() {
    let hideBelow = Infinity; // heading level that started the hiding
    for (const el of Array.from(this.preview.children) as HTMLElement[]) {
      const m = /^H([1-6])$/.exec(el.tagName);
      if (m) {
        const level = Number(m[1]);
        if (level <= hideBelow) hideBelow = Infinity;
        el.classList.toggle("is-hidden", hideBelow !== Infinity);
        if (hideBelow === Infinity && el.classList.contains("is-collapsed")) hideBelow = level;
        continue;
      }
      el.classList.toggle("is-hidden", hideBelow !== Infinity);
    }
  }

  private toggleHeading(el: HTMLElement) {
    const key = el.dataset.key ?? "";
    const folded = this.foldedLines();
    if (folded.has(key)) folded.delete(key);
    else folded.add(key);
    this.writeFolds(folded);
    el.classList.toggle("is-collapsed", folded.has(key));
    const caret = el.querySelector(".h-fold");
    if (caret) caret.setAttribute("title", folded.has(key) ? "Expand section" : "Collapse section");
    this.applyCollapse();
  }

  // ------------------------------------------------------- callouts, buttons

  /** Click on a callout's icon: pick another type. */
  private changeCallout(box: HTMLElement, x: number, y: number) {
    const doc = this.doc;
    const line = Number(box.dataset.line);
    if (!doc || Number.isNaN(line)) return;
    showMenu(
      x,
      y,
      CALLOUTS.map((c) => ({
        label: c.label,
        icon: c.icon,
        run: () => {
          const lines = doc.text.split("\n");
          const next = (lines[line] ?? "").replace(/\[![\w-]+\]/, `[!${c.type}]`);
          if (next === lines[line]) return;
          lines[line] = next;
          replaceText(doc, lines.join("\n"));
        },
      })),
    );
  }

  private async pressButton(el: HTMLElement) {
    const doc = this.doc;
    if (!doc) return;
    let spec: ButtonSpec;
    try {
      spec = JSON.parse(el.dataset.button ?? "{}") as ButtonSpec;
    } catch {
      return;
    }
    if (spec.action !== "append") {
      this.app.runButton(spec, this);
      return;
    }
    // Append: the template goes right below the button's block.
    const text = spec.template ? await this.app.templateText(spec.template, displayName(doc.path)) : "";
    const start = Number(el.closest<HTMLElement>("[data-line]")?.dataset.line);
    const lines = doc.text.split("\n");
    let end = start + 1;
    while (end < lines.length && !/^\s*(```|~~~)\s*$/.test(lines[end])) end++;
    lines.splice(end + 1, 0, "", ...text.replace(/\s+$/, "").split("\n"));
    replaceText(doc, lines.join("\n"));
  }

  // ----------------------------------------------------------------- header

  private renderHead() {
    const doc = this.doc;
    if (!doc) return;
    const folder = parentOf(doc.path);
    this.crumbPath.textContent = folder ? folder.split("/").join(" / ") : "Vault";
    this.crumbPath.title = folder || "The vault's top folder";
    if (document.activeElement !== this.title) this.title.textContent = displayName(doc.path);
    const nav = this.app.dailyNav(doc.path);
    this.daynav.hidden = !nav;
    this.daynav.replaceChildren();
    if (nav) {
      this.daynav.append(
        h(
          "button",
          { title: "Previous daily note", disabled: !nav.prev, onclick: () => nav.prev && this.app.openPath(nav.prev, this, false) },
          icon("caret-left"),
        ),
        h("span", null, nav.label),
        h(
          "button",
          { title: "Next daily note", disabled: !nav.next, onclick: () => nav.next && this.app.openPath(nav.next, this, false) },
          icon("caret-right"),
        ),
      );
    }
  }

  /** Tags, then "Edited … · N words · M backlinks". */
  renderMeta() {
    const doc = this.doc;
    if (!doc) return;
    const info = this.info;
    const parts: (HTMLElement | string)[] = [];
    for (const t of info?.tags ?? []) {
      parts.push(h("button.tag", { onclick: () => this.app.openTag(t.slice(1)), title: `Notes tagged ${t}` }, t));
    }
    const edited = doc.dirty ? "Edited just now" : info ? `Edited ${ago(info.modifiedMs)}` : "";
    const words = wordCount(doc.text);
    const back = info?.backlinks ?? 0;
    const stats = [edited, `${words} ${words === 1 ? "word" : "words"}`, `${back} ${back === 1 ? "backlink" : "backlinks"}`]
      .filter(Boolean)
      .join(" · ");
    parts.push(h("span.stats", null, stats));
    if (isLockedNote(doc.path) || lockedFolderOf(doc.path) !== null) {
      parts.unshift(
        h(
          "span.lock-chip",
          { title: "Locked with a password: this note is encrypted on disk. Use “Lock all notes now” (Ctrl+K) to close it until the password is entered again." },
          icon("lock-simple"),
          "Locked",
        ),
      );
    }
    this.meta.replaceChildren(...parts);
  }

  private bindTitle() {
    const t = this.title;
    let cancelled = false;
    const name = () => (t.textContent ?? "").replace(/\s+/g, " ").trim();
    const commit = async () => {
      const doc = this.doc;
      if (!doc) return;
      const current = displayName(doc.path);
      if (cancelled) {
        cancelled = false;
        t.textContent = current;
        return;
      }
      const next = name();
      if (!next || next === current) {
        t.textContent = current;
        return;
      }
      const ok = await this.app.rename(doc.path, next);
      if (!ok && this.doc) t.textContent = displayName(this.doc.path);
    };
    t.addEventListener("keydown", (e) => {
      if (e.key === "Enter" || (e.key === "ArrowDown" && atEnd(t))) {
        e.preventDefault();
        // On to the text, like moving down a line.
        this.mode = "source";
        this.applyMode();
        this.app.onModeChanged(this);
        this.view.focus();
      } else if (e.key === "Escape") {
        e.preventDefault();
        cancelled = true;
        this.focus();
      }
    });
    t.addEventListener("blur", () => void commit());
    // Pasting keeps to one plain line.
    t.addEventListener("paste", (e) => {
      e.preventDefault();
      const text = (e.clipboardData?.getData("text/plain") ?? "").replace(/\s+/g, " ");
      document.execCommand("insertText", false, text);
    });
  }

  // ---------------------------------------------------------------- preview

  renderPreview() {
    const doc = this.doc;
    if (!doc || this.mode !== "preview") return;
    if (this.renderedFor === doc.text) return this.restoreTop();
    this.renderedFor = doc.text;
    if (!doc.text.trim()) {
      this.preview.replaceChildren(
        h(
          "p.preview-empty",
          { style: "cursor:pointer", onclick: () => this.setMode("source") },
          `Empty note. Switch to Source (${kbd("E")}) to start writing.`,
        ),
      );
      return;
    }
    this.blocks.hide();
    this.preview.innerHTML = renderMarkdown(doc.text, this.unresolved);
    // A first "# Heading" that repeats the title shows once, as the title.
    const first = this.preview.firstElementChild;
    if (first?.tagName === "H1" && first.textContent?.trim().toLowerCase() === doc.name.trim().toLowerCase()) first.classList.add("is-title");
    this.decoratePreview();
    this.tables.decorate();
    for (const w of this.preview.querySelectorAll<HTMLElement>(":scope > .mermaid-wrap[data-line]")) {
      w.append(
        h(
          "button.mmd-edit",
          { onclick: () => this.editDiagramAt(Number(w.dataset.line)) },
          icon("pencil-simple"),
          "Edit diagram",
        ),
      );
    }
    this.restoreTop();
    void this.hydrate(this.preview, doc.path);
  }

  /** Puts the reader back where they were before an edit's redraw. */
  private restoreTop() {
    if (this.keepTop === null) return;
    const top = this.keepTop;
    this.keepTop = null;
    this.scroller.scrollTop = top;
    requestAnimationFrame(() => (this.scroller.scrollTop = top));
  }

  private assetUrl(path: string): string {
    const root = this.app.vaultRoot();
    const sep = root.includes("\\") ? "\\" : "/";
    return convertFileSrc(root.replace(/[\\/]+$/, "") + sep + path.split("/").join(sep));
  }

  /** The fenced block starting at `line`: its body lines. */
  private fenceAt(lines: string[], line: number): { start: number; end: number } | null {
    const open = /^\s*(`{3,}|~{3,})/.exec(lines[line] ?? "");
    if (!open) return null;
    let end = line + 1;
    while (end < lines.length && !new RegExp(`^\\s*${open[1][0] === "`" ? "`" : "~"}{${open[1].length},}\\s*$`).test(lines[end])) end++;
    return { start: line + 1, end };
  }

  /** Opens the diagram editor on the ```mermaid block at `line`. */
  editDiagramAt(line: number) {
    const doc = this.doc;
    if (!doc) return;
    const f = this.fenceAt(doc.text.split("\n"), line);
    if (!f) return;
    const body = doc.text.split("\n").slice(f.start, f.end).join("\n");
    openDiagramEditor(body, (next) => {
      const d = this.doc;
      if (!d) return;
      const lines = d.text.split("\n");
      const g = this.fenceAt(lines, line);
      if (!g) return;
      lines.splice(g.start, g.end - g.start, ...next.split("\n"));
      replaceText(d, lines.join("\n"));
    });
  }

  /** Rewrites one key in the ```database block at `line`. */
  private setDbConfig(line: number, key: string, value: unknown) {
    const doc = this.doc;
    if (!doc) return;
    const lines = doc.text.split("\n");
    if (!/^\s*(```|~~~)\s*database\s*$/i.test(lines[line] ?? "")) return;
    let end = line + 1;
    while (end < lines.length && !/^\s*(```|~~~)\s*$/.test(lines[end])) end++;
    const body = lines.slice(line + 1, end).join("\n");
    const next = setConfigKey(body, key, value);
    lines.splice(line + 1, end - line - 1, ...(next ? next.split("\n") : []));
    replaceText(doc, lines.join("\n"));
  }

  /** Fills in images and embedded notes, which need the vault. */
  /** Draws images, embeds, code and diagrams in `el` as Preview does (for slides). */
  async hydrateFor(el: HTMLElement): Promise<void> {
    if (!this.doc) return;
    await this.hydrate(el, this.doc.path);
    await highlightCodeBlocks(el);
    await renderMermaid(el);
  }

  private async hydrate(container: HTMLElement, source: string) {
    void highlightCodeBlocks(container);
    void renderMermaid(container);
    if (container === this.preview) {
      for (const box of container.querySelectorAll<HTMLElement>(".db[data-db]")) {
        const line = Number(box.dataset.line);
        new Database(box, box.dataset.db ?? "", {
          source,
          openPath: (path, inSplit) => this.app.openPath(path, this, inSplit),
          setConfig: (key, value) => this.setDbConfig(line, key, value),
          assetUrl: (p) => this.assetUrl(p),
        });
      }
    }
    for (const img of container.querySelectorAll<HTMLImageElement>("img[data-embed]")) {
      const target = img.dataset.embed!;
      const path = await api.resolveLink(target, source).catch(() => null);
      if (path) img.src = this.assetUrl(path);
      else img.replaceWith(h("span.embed-missing", null, `Missing image: ${target}`));
    }
    for (const media of container.querySelectorAll<HTMLMediaElement>("[data-embed-media]")) {
      const target = media.dataset.embedMedia!;
      const path = await api.resolveLink(target, source).catch(() => null);
      if (path) media.src = this.assetUrl(path);
      else media.replaceWith(h("span.embed-missing", null, `Missing recording: ${target}`));
    }
    for (const box of container.querySelectorAll<HTMLElement>("[data-embed-note]")) {
      const link = box.dataset.embedNote!;
      const target = linkTarget(link);
      const sub = link.includes("#") ? link.slice(link.indexOf("#") + 1) : "";
      const path = target ? await api.resolveLink(target, source).catch(() => null) : source;
      if (!path) {
        box.replaceWith(h("p.embed-missing", null, `Missing note: ${target}`));
        continue;
      }
      let locked = false;
      const text =
        findDoc(path)?.text ??
        (await api.readNote(path).catch((e) => {
          locked = String(e) === "LOCKED";
          return null;
        }));
      if (locked) {
        box.replaceWith(h("p.embed-missing", null, icon("lock-simple"), ` ${displayName(path)} is locked.`));
        continue;
      }
      if (text === null) continue;
      const range = embedRange(text, sub);
      if (!range) {
        box.replaceWith(h("p.embed-missing", null, `Missing ${sub.startsWith("^") ? "block" : "section"}: ${displayName(path)} › ${sub}`));
        continue;
      }
      const synced = sub.startsWith("^");
      box.dataset.path = path;
      box.classList.toggle("is-synced", synced);
      const label = synced ? `Synced from ${displayName(path)}` : displayName(path) + (sub ? ` › ${sub}` : "");
      // Only embeds in this note can be edited (not embeds inside embeds).
      const editable = container === this.preview;
      box.replaceChildren(
        h(
          "div.embed-title",
          null,
          h("span.embed-open", { dataset: { open: path }, title: `Open ${displayName(path)}` }, icon(synced ? "arrows-clockwise" : "file-text"), label),
          editable ? h("button.embed-edit", { title: "Edit here — changes the original everywhere", onclick: () => void this.editEmbed(box, path, sub) }, icon("pencil-simple"), "Edit") : "",
        ),
      );
      const body = h("div.embed-body");
      body.innerHTML = renderMarkdown(rangeText(text, range), new Set(), 1);
      // Lines and checkboxes belong to the other note.
      for (const el of body.querySelectorAll("[data-line]")) el.removeAttribute("data-line");
      for (const el of body.querySelectorAll("[data-task]")) el.removeAttribute("data-task");
      box.append(body);
      void this.hydrate(body, path);
    }
  }

  /** Edits an embed in place; saving changes the note it comes from. */
  private async editEmbed(box: HTMLElement, path: string, sub: string) {
    const text = findDoc(path)?.text ?? (await api.readNote(path).catch(() => null));
    const range = text === null ? null : embedRange(text, sub);
    if (text === null || !range) return;
    const original = rangeText(text, range);
    const area = h("textarea.embed-input", { spellcheck: true }) as HTMLTextAreaElement;
    area.value = original;
    const fit = () => {
      area.style.height = "auto";
      area.style.height = `${area.scrollHeight + 2}px`;
    };
    area.addEventListener("input", fit);
    const body = box.querySelector(".embed-body");
    const bar = h(
      "div.embed-actions",
      null,
      h("span.embed-hint", null, `${kbd("Enter")} to save · Esc to cancel`),
      h("button.btn.btn-secondary", { onclick: () => done(false) }, "Cancel"),
      h("button.btn.btn-primary", { onclick: () => void done(true) }, "Save"),
    );
    body?.replaceWith(area);
    box.querySelector(".embed-edit")?.setAttribute("hidden", "");
    box.append(bar);
    box.classList.add("is-editing");
    fit();
    area.focus();
    let finished = false;
    const done = async (save: boolean) => {
      if (finished) return;
      finished = true;
      box.classList.remove("is-editing");
      if (save && area.value !== original) {
        try {
          await editNote(path, (now) => {
            const r = embedRange(now, sub);
            if (!r) return now;
            const current = rangeText(now, r);
            // Changed meanwhile somewhere else: keep both changes.
            const edited = current === original ? area.value : mergeLines(original, area.value, current);
            return replaceRange(now, r, edited);
          });
        } catch (e) {
          this.app.toast(`Couldn't save the change to “${displayName(path)}”: ${e}`, "error");
        }
      }
      this.renderedFor = null;
      this.renderPreview();
    };
    area.addEventListener("keydown", (e) => {
      e.stopPropagation();
      if (e.key === "Escape") {
        e.preventDefault();
        void done(false);
      } else if (e.key === "Enter" && (e.ctrlKey || e.metaKey)) {
        e.preventDefault();
        void done(true);
      }
    });
  }

  /** Block handle → "Copy as synced block": gives the block an id, copies its embed. */
  private async copySynced(el: HTMLElement, start: number, end: number, li: boolean) {
    const doc = this.doc;
    if (!doc) return;
    let link: string;
    const name = displayName(doc.path);
    const hm = /^H[1-6]$/.test(el.tagName) ? /^#{1,6}\s+(.*?)\s*#*\s*$/.exec(doc.text.split("\n")[start] ?? "") : null;
    if (hm) {
      // A heading: the section under it.
      link = `![[${name}#${hm[1]}]]`;
    } else {
      const kind = li ? "li" : el.tagName === "P" ? "p" : "other";
      const r = ensureBlockId(doc.text, start, end, kind);
      if (r.text !== doc.text) replaceText(doc, r.text);
      link = `![[${name}#^${r.id}]]`;
    }
    try {
      await navigator.clipboard.writeText(link);
      this.app.toast("Synced block copied. Paste it into any note; editing it there changes it here too.");
    } catch {
      this.app.toast(`Copy this into another note: ${link}`);
    }
  }

  private bindPreview() {
    this.preview.addEventListener("click", (e) => {
      const t = e.target as HTMLElement;
      const split = e.ctrlKey || e.metaKey;
      const hf = t.closest<HTMLElement>(".h-fold");
      if (hf) {
        this.toggleHeading(hf.parentElement as HTMLElement);
        return;
      }
      const lf = t.closest<HTMLElement>(".li-fold");
      if (lf) {
        const shut = lf.parentElement?.classList.toggle("is-collapsed");
        lf.title = shut ? "Expand" : "Collapse";
        return;
      }
      const ci = t.closest<HTMLElement>(".callout-icon");
      if (ci) {
        e.preventDefault(); // don't also open/close a folded callout
        this.changeCallout(ci.closest<HTMLElement>("[data-callout]")!, e.clientX, e.clientY);
        return;
      }
      const btn = t.closest<HTMLElement>(".md-btn");
      if (btn) {
        void this.pressButton(btn);
        return;
      }
      const a = t.closest<HTMLElement>("a");
      // Footnotes: jump to the note and back.
      const fh = a?.getAttribute("href");
      if (a && fh?.startsWith("#fn")) {
        e.preventDefault();
        const target = this.preview.querySelector<HTMLElement>(`[id="${CSS.escape(fh.slice(1))}"]`);
        if (target) {
          this.scroller.scrollTop += target.getBoundingClientRect().top - this.scroller.getBoundingClientRect().top - 80;
          target.classList.remove("flash");
          void target.offsetWidth;
          target.classList.add("flash");
        }
        return;
      }
      if (a) {
        e.preventDefault();
        if (a.dataset.href) void openUrl(a.dataset.href).catch(() => {});
        else if (a.dataset.heading !== undefined) this.jumpToHeading(a.dataset.heading);
        else if (a.dataset.link !== undefined) {
          const link = a.dataset.link;
          if (link.startsWith("#") || linkTarget(link) === "") this.jumpToHeading(link.slice(link.indexOf("#") + 1));
          else this.app.openLink(linkTarget(link), this, split);
        }
        return;
      }
      const tag = t.closest<HTMLElement>("[data-tag]");
      if (tag) {
        this.app.openTag(tag.dataset.tag!);
        return;
      }
      const task = t.closest<HTMLElement>("[data-task]");
      if (task) {
        this.toggleTask(Number(task.dataset.task));
        return;
      }
      const open = t.closest<HTMLElement>("[data-open]");
      if (open) this.app.openPath(open.dataset.open!, this, split);
    });
    this.preview.addEventListener("dblclick", (e) => {
      const t = e.target as HTMLElement;
      const mw = t.closest<HTMLElement>(".mermaid-wrap[data-line]");
      if (mw && mw.parentElement === this.preview) {
        window.getSelection()?.removeAllRanges();
        this.editDiagramAt(Number(mw.dataset.line));
        return;
      }
      if (t.closest("a, [data-task], [data-tag], .embed, .h-fold, .li-fold, .md-btn, summary, td, th, .tbl-add, .db")) return;
      const block = t.closest<HTMLElement>("[data-line]");
      window.getSelection()?.removeAllRanges();
      this.setMode("source", block ? Number(block.dataset.line) : undefined);
    });
  }

  private jumpToHeading(text: string) {
    const want = text.trim().toLowerCase();
    const hd = headings(this.doc?.text ?? "").find((x) => x.text.trim().toLowerCase() === want);
    if (hd) this.scrollToLine(hd.line, true);
  }

  private toggleTask(line: number) {
    const doc = this.doc;
    if (!doc) return;
    const lines = doc.text.split("\n");
    const l = lines[line];
    if (l === undefined) return;
    const next = l.replace(/^(\s*(?:[-*+]|\d+[.)])\s+\[)([ xX])(\])/, (_m, a: string, c: string, b: string) => a + (c === " " ? "x" : " ") + b);
    if (next === l) return;
    lines[line] = next;
    replaceText(doc, lines.join("\n"));
  }
}

function atEnd(el: HTMLElement): boolean {
  const sel = window.getSelection();
  if (!sel || sel.rangeCount === 0) return true;
  const r = sel.getRangeAt(0).cloneRange();
  r.selectNodeContents(el);
  r.setStart(sel.getRangeAt(0).endContainer, sel.getRangeAt(0).endOffset);
  return r.toString().length === 0;
}

/** Two edits of the same text, line by line: both kept, nothing lost. */
function mergeLines(base: string, mine: string, theirs: string): string {
  if (mine === base) return theirs;
  if (theirs === base) return mine;
  const b = new Set(base.split("\n"));
  // Lines the other side added that this edit doesn't have.
  const added = theirs.split("\n").filter((l) => !b.has(l) && !mine.split("\n").includes(l));
  return added.length ? `${mine}\n${added.join("\n")}` : mine;
}
