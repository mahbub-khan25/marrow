// "Move from Notion / Evernote": a guided switch on top of the importers.
// It shows exactly how to export, finds the export in Downloads by itself,
// brings everything in, then explains where things ended up.

import { listen } from "@tauri-apps/api/event";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import { openUrl } from "@tauri-apps/plugin-opener";

import { api, type FoundExport } from "./api";
import { h, icon } from "./util";

export type SwitchFrom = "notion" | "evernote";

interface Guide {
  name: string;
  icon: string;
  folder: string;
  url: string;
  steps: [string, string][];
  /** [in the other app, in Marrow] */
  map: [string, string][];
  filters: { name: string; extensions: string[] }[];
  file: string;
}

export const GUIDES: Record<SwitchFrom, Guide> = {
  notion: {
    name: "Notion",
    icon: "notion-logo",
    folder: "Notion",
    url: "https://www.notion.so",
    file: "the .zip from Notion",
    steps: [
      ["Open the export", "In Notion, open Settings in the sidebar and choose Export all workspace content (on the General tab). For just one page and everything under it, use that page's ••• menu → Export."],
      ["Pick these options", "Export format: Markdown & CSV. Include everything, files and images too. If you see Include subpages or Create folders for subpages, turn them on."],
      ["Save the .zip", "Notion makes one .zip file. For a large workspace it emails you a download link instead. Don't unzip it: Marrow reads it as it is."],
    ],
    map: [
      ["Pages", "Notes"],
      ["Sub-pages", "Folders, with links"],
      ["Databases", "Database views, same columns"],
      ["Links and @-mentions of pages", "[[Links]] that keep working"],
      ["Properties", "Note properties"],
      ["Pictures and files", "Copied next to the notes"],
    ],
    filters: [{ name: "Notion export", extensions: ["zip"] }],
  },
  evernote: {
    name: "Evernote",
    icon: "notebook",
    folder: "Evernote",
    url: "https://www.evernote.com/client/web",
    file: "the .enex files",
    steps: [
      ["Export a notebook", "In Evernote, find the notebook in the sidebar, open its ••• menu (or right-click it) and choose Export notebook…"],
      ["Choose ENEX", "Pick the ENEX format (.enex), keep tags and dates, and save the file."],
      ["Repeat for each notebook", "Each notebook is one .enex file. You can bring in several at once in the next step."],
    ],
    map: [
      ["Notebooks", "Folders"],
      ["Notes", "Notes"],
      ["Tags", "Tags"],
      ["Checklists", "To-dos you can tick"],
      ["Created and updated dates", "Note properties"],
      ["Pictures and attachments", "Copied next to the notes"],
    ],
    filters: [{ name: "Evernote export", extensions: ["enex"] }],
  },
};

export interface SwitchApp {
  /** After the import: show the folder. */
  done(folder: string): void;
  mobile: boolean;
}

const size = (b: number) => (b > 1e9 ? `${(b / 1e9).toFixed(1)} GB` : b > 1e6 ? `${Math.round(b / 1e6)} MB` : `${Math.max(1, Math.round(b / 1e3))} KB`);
const when = (t: number) => {
  const days = Math.floor((Date.now() - t) / 86_400_000);
  return days === 0 ? "today" : days === 1 ? "yesterday" : `${days} days ago`;
};

export function openSwitch(app: SwitchApp, from: SwitchFrom) {
  const g = GUIDES[from];
  const body = h("div.sw-body");
  const card = h("div.dlg.sw", { role: "dialog", "aria-label": `Move from ${g.name}` }, body);
  const wrap = h("div.backdrop.dlg-wrap", null, card);
  let busy = false;
  let unDrop: (() => void) | null = null;
  const close = () => {
    if (busy) return;
    unDrop?.();
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

  const head = (step: number, title: string) =>
    h(
      "div.sw-head",
      null,
      h("div.sw-steps", { "aria-label": `Step ${step} of 3` }, ...[1, 2, 3].map((n) => h("span", { "data-on": String(n <= step) }))),
      h("div.sw-row", null, h("h2", null, title), h("button.icon-btn", { title: "Close (Esc)", "aria-label": "Close", onclick: close }, icon("x"))),
    );

  // ---- 1: how to export
  function guide() {
    body.replaceChildren(
      head(1, `Move from ${g.name}`),
      h("p.dlg-lede", null, `Everything comes across as plain Markdown files you own. First, export from ${g.name}; it takes a minute.`),
      h(
        "ol.sw-guide",
        null,
        ...g.steps.map(([t, d], i) => h("li", null, h("span.n", null, String(i + 1)), h("div", null, h("b", null, t), h("p", null, d)))),
      ),
      h(
        "details.sw-map",
        null,
        h("summary", null, "What comes across"),
        h("table", null, h("thead", null, h("tr", null, h("th", null, `In ${g.name}`), h("th", null, "In Marrow"))), h("tbody", null, ...g.map.map(([a, b]) => h("tr", null, h("td", null, a), h("td", null, b))))),
      ),
      h(
        "div.dlg-foot",
        null,
        h("button.btn.btn-secondary", { onclick: () => void openUrl(g.url).catch(() => {}) }, icon("arrow-square-out"), `Open ${g.name}`),
        h("button.btn.btn-primary", { onclick: () => void pick() }, "I have the file", icon("arrow-right")),
      ),
    );
  }

  // ---- 2: pick the file (found in Downloads, chosen, or dropped)
  async function pick() {
    const found: FoundExport[] = app.mobile ? [] : await api.findExports(from).catch(() => []);
    const list = found.length
      ? h(
          "div.sw-found",
          null,
          h("div.sw-k", null, icon("download-simple"), "Found in your Downloads"),
          ...found.map((f) =>
            h(
              "button.sw-file",
              { onclick: () => void run([f.path]) },
              icon(from === "notion" ? "file-zip" : "file-text"),
              h("span.n", { translate: "no" }, f.name),
              h("span.m", null, `${size(f.size)} · ${when(f.modifiedMs)}`),
              h("span.go", null, "Bring it in", icon("arrow-right")),
            ),
          ),
        )
      : null;
    const drop = h(
      "div.sw-drop",
      null,
      icon("tray-arrow-down"),
      h("p", null, app.mobile ? `Choose ${g.file}.` : `Drop ${g.file} here, or choose it.`),
      h("button.btn.btn-secondary", { onclick: () => void choose() }, "Choose the file…"),
    );
    body.replaceChildren(
      head(2, "Bring it in"),
      h("p.dlg-lede", null, `The notes go into a new “${g.folder}” folder in this vault. The export itself is only read, never changed.`),
      list ?? h("span"),
      drop,
      h("div.dlg-foot", null, h("button.btn.btn-ghost", { onclick: guide }, icon("arrow-left"), "Back")),
    );
    if (!app.mobile && !unDrop) {
      try {
        const { getCurrentWebview } = await import("@tauri-apps/api/webview");
        unDrop = await getCurrentWebview().onDragDropEvent((e) => {
          if (busy || !wrap.isConnected) return;
          if (e.payload.type === "over" || e.payload.type === "enter") drop.classList.add("is-over");
          else if (e.payload.type === "leave") drop.classList.remove("is-over");
          else if (e.payload.type === "drop") {
            drop.classList.remove("is-over");
            const ext = g.filters[0].extensions;
            const paths = e.payload.paths.filter((p) => ext.some((x) => p.toLowerCase().endsWith("." + x)));
            if (paths.length) void run(paths);
          }
        });
      } catch {
        // Not running in the app (a preview): choosing works.
      }
    }
  }

  async function choose() {
    const picked = await openDialog({ title: `Choose ${g.file}`, multiple: true, filters: g.filters });
    const paths = Array.isArray(picked) ? picked : picked ? [picked] : [];
    if (paths.length) await run(paths);
  }

  // ---- 3: import, then where things are
  async function run(paths: string[]) {
    busy = true;
    const status = h("div.imp-status", null, h("span.ai-dots"), " Reading the export…");
    const meter = h("div.ai-meter", null, h("span", { style: "width:4%" }));
    body.replaceChildren(head(3, `Bringing in your ${g.name} notes`), meter, status);
    const unlisten = await listen<{ done?: number; total?: number; current?: string; writing?: number }>("import-progress", (e) => {
      const p = e.payload;
      if (p.writing !== undefined) {
        (meter.firstElementChild as HTMLElement).style.width = "92%";
        status.replaceChildren(h("span.ai-dots"), ` Writing ${p.writing.toLocaleString()} files…`);
      } else if (p.total) {
        (meter.firstElementChild as HTMLElement).style.width = `${Math.max(4, ((p.done ?? 0) / p.total) * 80)}%`;
        status.replaceChildren(h("span.ai-dots"), ` ${p.current ?? ""}`);
      }
    }).catch(() => () => {});
    try {
      const r = await api.importFiles(from, paths, g.folder);
      busy = false;
      body.replaceChildren(
        head(3, "Welcome to Marrow"),
        h(
          "p.imp-done",
          null,
          icon("check-circle"),
          ` ${r.notes.toLocaleString()} ${r.notes === 1 ? "note" : "notes"}${r.attachments ? ` and ${r.attachments.toLocaleString()} ${r.attachments === 1 ? "file" : "files"}` : ""} from ${g.name} are in “${r.folder}”.`,
        ),
        h(
          "ul.sw-tips",
          null,
          h("li", null, "Everything is plain Markdown in your vault folder, so it opens in any app, now and in twenty years."),
          h("li", null, from === "notion" ? "Type / on an empty line for headings, to-dos, tables, callouts and databases, as in Notion." : "Press Ctrl+Alt+N anywhere to jot a note, the way you'd clip to Evernote."),
          h("li", null, "Links between your notes came across, so backlinks and the graph already work."),
        ),
        r.warnings.length
          ? h("details.imp-warn", null, h("summary", null, `${r.warnings.length} ${r.warnings.length === 1 ? "thing" : "things"} didn't come across`), h("ul", null, ...r.warnings.map((w) => h("li", null, w))))
          : h("span"),
        h(
          "div.dlg-foot",
          null,
          h("button.btn.btn-secondary", { onclick: () => void pick() }, "Bring in more"),
          h("button.btn.btn-primary", { onclick: () => (close(), app.done(r.folder)) }, "Show my notes"),
        ),
      );
    } catch (e) {
      busy = false;
      body.replaceChildren(
        head(3, "That didn't work"),
        h("p.imp-error", null, icon("warning-circle"), ` ${e}`),
        h("p.dlg-lede", null, from === "notion" ? "Check that the file is Notion's Markdown & CSV export (a .zip), not HTML or PDF." : "Check that the file is an .enex export from Evernote."),
        h("div.dlg-foot", null, h("button.btn.btn-secondary", { onclick: guide }, "See the steps again"), h("button.btn.btn-primary", { onclick: () => void pick() }, "Try another file")),
      );
    } finally {
      unlisten();
    }
  }

  guide();
}
