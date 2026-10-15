// Import from Notion, Evernote, Word/OneNote and web pages. The files are
// converted to Markdown notes in a new folder; the originals are untouched.

import { listen } from "@tauri-apps/api/event";
import { open as openDialog } from "@tauri-apps/plugin-dialog";

import { api } from "./api";
import { h, icon } from "./util";

export interface ImportApp {
  /** After an import: show the new folder. */
  done(folder: string): void;
}

interface Source {
  kind: string;
  name: string;
  icon: string;
  folder: string;
  how: string;
  filters: { name: string; extensions: string[] }[];
  multiple: boolean;
}

const SOURCES: Source[] = [
  {
    kind: "notion",
    name: "Notion",
    icon: "notion-logo",
    folder: "Notion",
    how: "In Notion: ⋯ → Export → “Markdown & CSV”, include subpages, then choose the .zip it saves. Pages, links, pictures and databases come across.",
    filters: [{ name: "Notion export", extensions: ["zip"] }],
    multiple: true,
  },
  {
    kind: "evernote",
    name: "Evernote",
    icon: "notebook",
    folder: "Evernote",
    how: "In Evernote: select a notebook's notes → File → Export Notes → .enex. Each .enex becomes a folder; tags, dates, pictures and checkboxes come across.",
    filters: [{ name: "Evernote export", extensions: ["enex"] }],
    multiple: true,
  },
  {
    kind: "word",
    name: "OneNote or Word",
    icon: "microsoft-word-logo",
    folder: "OneNote",
    how: "In OneNote: File → Export → Page or Section → Word Document (.docx). Any Word file works too. Headings, lists, tables, links and pictures come across.",
    filters: [{ name: "Word document", extensions: ["docx"] }],
    multiple: true,
  },
  {
    kind: "html",
    name: "Web pages or Markdown",
    icon: "globe-simple",
    folder: "Imported",
    how: "Saved web pages (.html) with their pictures, or Markdown and text files from other apps.",
    filters: [{ name: "Pages and notes", extensions: ["html", "htm", "md", "markdown", "txt"] }],
    multiple: true,
  },
];

export function openImport(app: ImportApp) {
  const body = h("div.imp-body");
  const card = h("div.dlg.imp", { role: "dialog", "aria-label": "Import" }, h("h2", null, "Import notes"), body);
  const wrap = h("div.backdrop.dlg-wrap", null, card);
  let busy = false;
  const close = () => {
    if (busy) return;
    wrap.remove();
    document.removeEventListener("keydown", keys, true);
    document.documentElement.toggleAttribute("data-modal", false);
  };
  const keys = (e: KeyboardEvent) => {
    if (e.key === "Escape") {
      e.stopPropagation();
      close();
    }
  };
  wrap.addEventListener("mousedown", (e) => e.target === wrap && close());
  document.addEventListener("keydown", keys, true);
  document.body.append(wrap);
  document.documentElement.toggleAttribute("data-modal", true);

  const choose = () => {
    body.replaceChildren(
      h("p.dlg-lede", null, "Your notes are converted to Markdown in a new folder of this vault. The files you pick are only read, never changed."),
      h(
        "div.imp-sources",
        null,
        ...SOURCES.map((s) =>
          h(
            "button.imp-source",
            { onclick: () => void pick(s) },
            h("span.imp-icon", null, icon(s.icon)),
            h("span.imp-text", null, h("span.imp-name", null, s.name), h("span.imp-how", null, s.how)),
          ),
        ),
      ),
      h("div.dlg-foot", null, h("button.btn.btn-secondary", { onclick: close }, "Cancel")),
    );
  };

  async function pick(s: Source) {
    const picked = await openDialog({ title: `Import from ${s.name}`, multiple: s.multiple, filters: s.filters });
    const paths = Array.isArray(picked) ? picked : picked ? [picked] : [];
    if (!paths.length) return;
    busy = true;
    const status = h("div.imp-status", null, h("span.ai-dots"), " Reading…");
    const meter = h("div.ai-meter", null, h("span", { style: "width:4%" }));
    body.replaceChildren(h("p.dlg-lede", null, `Importing from ${s.name}…`), meter, status);
    const unlisten = await listen<{ done?: number; total?: number; current?: string; writing?: number }>("import-progress", (e) => {
      const p = e.payload;
      if (p.writing !== undefined) status.replaceChildren(h("span.ai-dots"), ` Writing ${p.writing} files…`);
      else if (p.total) {
        (meter.firstElementChild as HTMLElement).style.width = `${Math.max(4, ((p.done ?? 0) / p.total) * 100)}%`;
        status.replaceChildren(h("span.ai-dots"), ` ${p.current ?? ""}`);
      }
    });
    try {
      const r = await api.importFiles(s.kind, paths, s.folder);
      busy = false;
      body.replaceChildren(
        h("p.imp-done", null, icon("check-circle"), ` Imported ${r.notes} ${r.notes === 1 ? "note" : "notes"}${r.attachments ? ` and ${r.attachments} ${r.attachments === 1 ? "file" : "files"}` : ""} into “${r.folder}”.`),
        r.warnings.length
          ? h("details.imp-warn", null, h("summary", null, `${r.warnings.length} ${r.warnings.length === 1 ? "thing" : "things"} didn't come across`), h("ul", null, ...r.warnings.map((w) => h("li", null, w))))
          : h("span"),
        h(
          "div.dlg-foot",
          null,
          h("button.btn.btn-secondary", { onclick: () => choose() }, "Import more"),
          h(
            "button.btn.btn-primary",
            {
              onclick: () => {
                close();
                app.done(r.folder);
              },
            },
            "Show the notes",
          ),
        ),
      );
    } catch (e) {
      busy = false;
      body.replaceChildren(
        h("p.imp-error", null, icon("warning-circle"), ` ${e}`),
        h("div.dlg-foot", null, h("button.btn.btn-secondary", { onclick: close }, "Close"), h("button.btn.btn-primary", { onclick: () => choose() }, "Try again")),
      );
    } finally {
      unlisten();
    }
  }

  choose();
}
