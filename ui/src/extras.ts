// Customising Marrow from the vault: CSS snippets and plugins.
//
// * Snippets are `.css` files in `<vault>/.marrow/snippets`; the ones turned
//   on are added after Marrow's own styles.
// * Plugins are folders in `<vault>/.marrow/plugins` with `manifest.json` and
//   `main.js`. Each runs in its own sandboxed frame: it can't reach the
//   internet, Marrow's window or your files. It can only call the small
//   `marrow` API below, and only use what its manifest asks for and you
//   allowed when turning it on ("read" notes, "write" notes).
//
//     marrow.addCommand({ id, name, icon? }, fn)   → appears in Ctrl+K
//     await marrow.note()        → { path, title, text, selection } of the open note
//     await marrow.insert(text)  → replaces the selection / inserts at the cursor
//     await marrow.setText(text) → replaces the open note's text
//     await marrow.notes()       → [{ path, title }]
//     await marrow.read(path)    → a note's text
//     await marrow.create(name, text) → path of a new note
//     await marrow.open(path)    → opens a note
//     marrow.notice(text)        → a short message
//     await marrow.ask(question, defaultValue?) → what the person typed, or null

import { revealItemInDir } from "@tauri-apps/plugin-opener";

import { api, type PluginInfo } from "./api";
import { confirmDialog } from "./lock";
import { h, icon } from "./util";

export interface PluginHooks {
  /** The open note (the focused pane's), if any. */
  note(): { path: string; title: string; text: string; selection: string } | null;
  insert(text: string): boolean;
  setText(text: string): boolean;
  notes(): { path: string; title: string }[];
  /** A note's text; refuses locked notes. */
  read(path: string): Promise<string>;
  create(name: string, text: string): Promise<string>;
  open(path: string): void;
  toast(text: string, kind?: "info" | "error"): void;
}

// ---------------------------------------------------------------- snippets

let snippetNames: string[] = [];

/** Adds the snippets that are on (and removes the others). */
export async function applySnippets(on: string[]): Promise<string[]> {
  const list = await api.snippetsList().catch(() => []);
  snippetNames = list.map((s) => s.name);
  for (const el of document.querySelectorAll("style[data-snippet]")) el.remove();
  for (const s of list) {
    if (!on.includes(s.name)) continue;
    const style = document.createElement("style");
    style.dataset.snippet = s.name;
    style.textContent = s.css;
    document.head.append(style);
  }
  return snippetNames;
}

// ----------------------------------------------------------------- plugins

interface Loaded {
  dir: string;
  name: string;
  perms: Set<string>;
  frame: HTMLIFrameElement;
  commands: Map<string, { name: string; icon: string }>;
}

const loaded = new Map<string, Loaded>();
let hooks: PluginHooks | null = null;
let known: PluginInfo[] = [];

const BOOT = `(function () {
  var pending = new Map(), seq = 0, cmds = new Map();
  function send(m) { m.marrow = 1; parent.postMessage(m, "*"); }
  function call(method) {
    var args = Array.prototype.slice.call(arguments, 1);
    return new Promise(function (res, rej) { var id = ++seq; pending.set(id, { res: res, rej: rej }); send({ kind: "call", id: id, method: method, args: args }); });
  }
  window.addEventListener("message", function (e) {
    if (e.source !== parent) return;
    var m = e.data;
    if (!m || m.marrow !== 1) return;
    if (m.kind === "reply") {
      var p = pending.get(m.id); if (!p) return; pending.delete(m.id);
      if (m.error) p.rej(new Error(m.error)); else p.res(m.value);
    } else if (m.kind === "run") {
      var fn = cmds.get(m.command);
      Promise.resolve().then(function () { return fn && fn(); }).catch(function (err) { call("notice", "Plugin error: " + ((err && err.message) || err)); });
    }
  });
  window.addEventListener("error", function (e) { call("notice", "Plugin error: " + e.message); });
  window.marrow = Object.freeze({
    addCommand: function (spec, fn) {
      if (!spec || !spec.id || typeof fn !== "function") throw new Error("Use marrow.addCommand({ id, name }, function)");
      cmds.set(String(spec.id), fn);
      send({ kind: "command", id: String(spec.id), name: String(spec.name || spec.id), icon: String(spec.icon || "puzzle-piece") });
    },
    note: function () { return call("note"); },
    insert: function (t) { return call("insert", String(t)); },
    setText: function (t) { return call("setText", String(t)); },
    notes: function () { return call("notes"); },
    read: function (p) { return call("read", String(p)); },
    create: function (n, t) { return call("create", String(n), t == null ? "" : String(t)); },
    open: function (p) { return call("open", String(p)); },
    notice: function (t) { return call("notice", String(t)); },
    ask: function (q, d) { return call("ask", String(q), d == null ? "" : String(d)); }
  });
})();`;

/** What each permission lets a plugin do, in words. */
const PERMS: Record<string, string> = {
  read: "read your notes",
  write: "change your notes and create new ones",
};
const NEEDS: Record<string, string | null> = {
  note: "read",
  notes: "read",
  read: "read",
  insert: "write",
  setText: "write",
  create: "write",
  open: null,
  notice: null,
  ask: null,
};

export function initPlugins(h: PluginHooks) {
  hooks = h;
  window.addEventListener("message", (e) => {
    const p = [...loaded.values()].find((x) => x.frame.contentWindow === e.source);
    const m = e.data as { marrow?: number; kind?: string; id?: number | string; method?: string; args?: unknown[]; name?: string; icon?: string };
    if (!p || !m || m.marrow !== 1) return;
    if (m.kind === "command") {
      p.commands.set(String(m.id), { name: String(m.name).slice(0, 80), icon: /^[a-z-]{1,40}$/.test(String(m.icon)) ? String(m.icon) : "puzzle-piece" });
    } else if (m.kind === "call") {
      void handle(p, String(m.method), Array.isArray(m.args) ? m.args.map(String) : []).then(
        (value) => p.frame.contentWindow?.postMessage({ marrow: 1, kind: "reply", id: m.id, value }, "*"),
        (err) => p.frame.contentWindow?.postMessage({ marrow: 1, kind: "reply", id: m.id, error: String((err as Error)?.message ?? err) }, "*"),
      );
    }
  });
}

async function handle(p: Loaded, method: string, args: string[]): Promise<unknown> {
  if (!hooks || !(method in NEEDS)) throw new Error(`Unknown call: ${method}`);
  const need = NEEDS[method];
  if (need && !p.perms.has(need)) throw new Error(`“${p.name}” wasn't allowed to ${PERMS[need]}. Add "${need}" to its manifest's permissions.`);
  switch (method) {
    case "note":
      return hooks.note();
    case "notes":
      return hooks.notes();
    case "read":
      return hooks.read(args[0]);
    case "insert":
      if (!hooks.insert(args[0] ?? "")) throw new Error("Open a note first.");
      return true;
    case "setText":
      if (!hooks.setText(args[0] ?? "")) throw new Error("Open a note first.");
      return true;
    case "create":
      return hooks.create(args[0] || "Untitled", args[1] ?? "");
    case "open":
      hooks.open(args[0]);
      return true;
    case "notice":
      hooks.toast(`${p.name}: ${(args[0] ?? "").slice(0, 300)}`);
      return true;
    case "ask":
      return askText(p.name, args[0] ?? "", args[1] ?? "");
  }
  return null;
}

/** Starts the plugins that are on (and stops the others). */
export async function startPlugins(on: string[]): Promise<PluginInfo[]> {
  known = await api.pluginsList().catch(() => []);
  for (const [dir, p] of loaded) {
    if (!on.includes(dir) || !known.some((k) => k.dir === dir)) {
      p.frame.remove();
      loaded.delete(dir);
    }
  }
  for (const info of known) {
    if (!on.includes(info.dir) || loaded.has(info.dir) || info.problem || !info.code) continue;
    load(info);
  }
  return known;
}

function load(info: PluginInfo) {
  const name = info.manifest?.name || info.dir;
  const frame = document.createElement("iframe");
  // Scripts only: no same origin, so no access to Marrow's window or storage.
  frame.setAttribute("sandbox", "allow-scripts");
  frame.className = "plugin-frame";
  frame.setAttribute("aria-hidden", "true");
  frame.tabIndex = -1;
  const code = (info.code ?? "").replace(/<\/script/gi, "<\\/script");
  frame.srcdoc = `<!doctype html><meta charset="utf-8"><meta http-equiv="Content-Security-Policy" content="default-src 'none'; script-src 'unsafe-inline'"><script>${BOOT}</script><script>${code}</script>`;
  document.body.append(frame);
  loaded.set(info.dir, { dir: info.dir, name, perms: new Set(info.manifest?.permissions ?? []), frame, commands: new Map() });
}

/** Commands from the plugins that are on, for the palette. */
export function pluginCommands(): { label: string; icon: string; run: () => void }[] {
  const out: { label: string; icon: string; run: () => void }[] = [];
  for (const p of loaded.values()) {
    for (const [id, c] of p.commands) {
      out.push({ label: `${p.name}: ${c.name}`, icon: c.icon, run: () => p.frame.contentWindow?.postMessage({ marrow: 1, kind: "run", command: id }, "*") });
    }
  }
  return out;
}

/** A small "type something" box for plugins. */
function askText(from: string, question: string, value: string): Promise<string | null> {
  return new Promise((resolve) => {
    const input = h("input.input", { value, spellcheck: false }) as HTMLInputElement;
    const ok = h("button.btn.btn-primary", { type: "submit" }, "OK");
    const form = h(
      "form.dlg",
      { role: "dialog", "aria-label": from },
      h("h2", null, from),
      h("p.dlg-lede", null, question.slice(0, 400)),
      input,
      h("div.dlg-foot", null, h("button.btn.btn-secondary", { type: "button", onclick: () => close(null) }, "Cancel"), ok),
    ) as HTMLFormElement;
    form.addEventListener("submit", (e) => {
      e.preventDefault();
      close(input.value);
    });
    const wrap = h("div.backdrop.dlg-wrap", null, form);
    document.body.append(wrap);
    document.documentElement.toggleAttribute("data-modal", true);
    input.focus();
    input.select();
    const keys = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        e.stopPropagation();
        close(null);
      }
    };
    document.addEventListener("keydown", keys, true);
    function close(v: string | null) {
      wrap.remove();
      document.removeEventListener("keydown", keys, true);
      document.documentElement.toggleAttribute("data-modal", false);
      resolve(v);
    }
  });
}

// ------------------------------------------------------------ the settings

export interface CustomizeApp {
  snippetsOn(): string[];
  pluginsOn(): string[];
  /** Saves the vault setting and applies it. */
  setSnippets(on: string[]): void;
  setPlugins(on: string[]): void;
  toast(text: string, kind?: "info" | "error"): void;
}

const EXAMPLE_SNIPPET = `/* A CSS snippet for Marrow. Turn it on in Settings → Customize.
   Change anything you like; press Reload there to see it.

   Useful variables: --note-size, --note-width, --font-body, --font-heading,
   --color-accent, --color-bg, --color-text. */

/* Headings in the accent colour */
.preview h1, .preview h2, .preview h3 { color: var(--color-accent-700); }

/* A little more room between paragraphs */
.preview p { margin-block: 0.9em; }
`;

const EXAMPLE_MANIFEST = `{
  "id": "writing-tools",
  "name": "Writing tools",
  "version": "1.0.0",
  "description": "Insert the date, sort lines, change case and count words. An example to copy.",
  "author": "Marrow",
  "permissions": ["read", "write"]
}
`;

const EXAMPLE_PLUGIN = `// An example plugin. It runs in a sandbox and talks to Marrow only through
// the \`marrow\` object. Edit it, then press Reload in Settings → Customize.

marrow.addCommand({ id: "date", name: "Insert date and time", icon: "calendar-blank" }, async () => {
  const d = new Date();
  const pad = (n) => String(n).padStart(2, "0");
  await marrow.insert(\`\${d.getFullYear()}-\${pad(d.getMonth() + 1)}-\${pad(d.getDate())} \${pad(d.getHours())}:\${pad(d.getMinutes())}\`);
});

marrow.addCommand({ id: "sort", name: "Sort selected lines", icon: "sort-ascending" }, async () => {
  const note = await marrow.note();
  if (!note || !note.selection) return marrow.notice("Select some lines first (in Source).");
  const lines = note.selection.split("\\n").sort((a, b) => a.localeCompare(b, undefined, { numeric: true, sensitivity: "base" }));
  await marrow.insert(lines.join("\\n"));
});

marrow.addCommand({ id: "title-case", name: "Title Case the selection", icon: "text-aa" }, async () => {
  const note = await marrow.note();
  if (!note || !note.selection) return marrow.notice("Select some text first (in Source).");
  const small = new Set(["a", "an", "and", "as", "at", "but", "by", "for", "in", "of", "on", "or", "the", "to", "with"]);
  const out = note.selection.toLowerCase().replace(/[\\p{L}']+/gu, (w, i) => (i > 0 && small.has(w) ? w : w[0].toUpperCase() + w.slice(1)));
  await marrow.insert(out);
});

marrow.addCommand({ id: "count", name: "Count words in this note", icon: "hash" }, async () => {
  const note = await marrow.note();
  if (!note) return marrow.notice("Open a note first.");
  const words = note.text.replace(/^---[\\s\\S]*?\\n---\\n/, "").split(/\\s+/).filter(Boolean).length;
  marrow.notice(\`“\${note.title}” has \${words} words.\`);
});
`;

/** Settings → Customize. */
export async function renderCustomize(body: HTMLElement, rerender: () => void, app: CustomizeApp) {
  const snippets = await api.snippetsList().catch(() => []);
  const plugins = await api.pluginsList().catch(() => [] as PluginInfo[]);
  const reveal = async (kind: "snippets" | "plugins") => {
    const dir = await api.extrasDir(kind);
    await revealItemInDir(dir).catch((e) => app.toast(String(e), "error"));
  };
  const toggle = (on: boolean, set: (v: boolean) => void, label: string) =>
    h("button.switch", { type: "button", role: "switch", "aria-checked": String(on), "aria-label": label, onclick: () => set(!on) }, h("span"));

  const snipRows = snippets.length
    ? snippets.map((s) => {
        const on = app.snippetsOn().includes(s.name);
        return h(
          "div.ext-row",
          null,
          icon("paint-brush"),
          h("div.ext-main", null, h("div.ext-name", null, s.name), h("div.ext-desc", null, firstComment(s.css))),
          toggle(
            on,
            (v) => {
              app.setSnippets(v ? [...app.snippetsOn(), s.name] : app.snippetsOn().filter((n) => n !== s.name));
              rerender();
            },
            `Use ${s.name}`,
          ),
        );
      })
    : [h("p.ext-empty", null, "No snippets yet. Add a .css file to the snippets folder, or start from the example.")];

  const plugRows = plugins.length
    ? plugins.map((p) => {
        const on = app.pluginsOn().includes(p.dir);
        const m = p.manifest;
        const perms = (m?.permissions ?? []).filter((x) => x in PERMS);
        return h(
          "div.ext-row",
          null,
          icon("puzzle-piece"),
          h(
            "div.ext-main",
            null,
            h("div.ext-name", null, m?.name || p.dir, m?.version ? h("span.ext-ver", null, ` ${m.version}`) : ""),
            h("div.ext-desc", null, p.problem ? h("span.ext-problem", null, p.problem) : (m?.description ?? "")),
            perms.length ? h("div.ext-perms", null, ...perms.map((x) => h("span.badge", null, x === "read" ? "Reads notes" : "Changes notes"))) : "",
          ),
          p.problem
            ? ""
            : toggle(
                on,
                (v) =>
                  void (async () => {
                    if (v && perms.length) {
                      const ok = await confirmDialog(
                        `Turn on “${m?.name || p.dir}”?`,
                        `This plugin asks to ${perms.map((x) => PERMS[x]).join(" and ")}. It runs in a sandbox with no internet access. Only turn on plugins you trust.`,
                        "Turn on",
                      );
                      if (!ok) return;
                    }
                    app.setPlugins(v ? [...app.pluginsOn(), p.dir] : app.pluginsOn().filter((n) => n !== p.dir));
                    rerender();
                  })(),
                `Use ${m?.name || p.dir}`,
              ),
        );
      })
    : [h("p.ext-empty", null, "No plugins yet. Add one to the plugins folder, or start from the example.")];

  body.append(
    h(
      "section.ext-sec",
      null,
      h("h3", null, "CSS snippets"),
      h("p.ext-lede", null, "Restyle Marrow with your own CSS. Snippets are .css files kept in this vault (", h("code", null, ".marrow/snippets"), "), so they travel with it."),
      h(
        "div.ext-actions",
        null,
        h("button.btn.btn-secondary", { type: "button", onclick: () => void reveal("snippets") }, icon("folder-open"), "Open folder"),
        h(
          "button.btn.btn-secondary",
          {
            type: "button",
            onclick: () =>
              void api.extrasWrite("snippets", "my-style.css", EXAMPLE_SNIPPET).then((made) => {
                app.toast(made ? "Added the example snippet “my-style”." : "“my-style.css” is already there.");
                rerender();
              }),
          },
          icon("plus"),
          "Example snippet",
        ),
        h("button.btn.btn-secondary", { type: "button", onclick: () => (app.setSnippets(app.snippetsOn()), rerender()) }, icon("arrows-clockwise"), "Reload"),
      ),
      h("div.ext-list", null, ...snipRows),
    ),
    h(
      "section.ext-sec",
      null,
      h("h3", null, "Plugins"),
      h(
        "p.ext-lede",
        null,
        "Plugins add commands to the command palette (Ctrl+K). Each one runs in a sandbox: no internet, no access to your files, only what you allow when you turn it on. They're kept in ",
        h("code", null, ".marrow/plugins"),
        ".",
      ),
      h(
        "div.ext-actions",
        null,
        h("button.btn.btn-secondary", { type: "button", onclick: () => void reveal("plugins") }, icon("folder-open"), "Open folder"),
        h(
          "button.btn.btn-secondary",
          {
            type: "button",
            onclick: () =>
              void (async () => {
                const a = await api.extrasWrite("plugins", "writing-tools/manifest.json", EXAMPLE_MANIFEST);
                await api.extrasWrite("plugins", "writing-tools/main.js", EXAMPLE_PLUGIN);
                app.toast(a ? "Added the example plugin “Writing tools”. Turn it on to try it." : "The example plugin is already there.");
                rerender();
              })(),
          },
          icon("plus"),
          "Example plugin",
        ),
        h("button.btn.btn-secondary", { type: "button", onclick: () => (reloadPlugins(app), rerender()) }, icon("arrows-clockwise"), "Reload"),
      ),
      h("div.ext-list", null, ...plugRows),
      h(
        "details.ext-help",
        null,
        h("summary", null, "Writing a plugin"),
        h(
          "pre",
          null,
          `.marrow/plugins/my-plugin/manifest.json
  { "id": "my-plugin", "name": "My plugin", "version": "1.0.0",
    "description": "…", "permissions": ["read", "write"] }

.marrow/plugins/my-plugin/main.js
  marrow.addCommand({ id: "hello", name: "Say hello" }, async () => {
    const note = await marrow.note();          // needs "read"
    await marrow.insert("Hello from " + note.title);  // needs "write"
  });

Also: marrow.setText(text), marrow.notes(), marrow.read(path),
marrow.create(name, text), marrow.open(path), marrow.notice(text),
marrow.ask(question, default)`,
        ),
      ),
    ),
  );
}

/** Restarts every plugin that's on (after editing its code). */
function reloadPlugins(app: CustomizeApp) {
  for (const p of loaded.values()) p.frame.remove();
  loaded.clear();
  app.setPlugins(app.pluginsOn());
}

function firstComment(css: string): string {
  const m = /\/\*\s*([\s\S]*?)\*\//.exec(css);
  const line = m?.[1].split("\n")[0].trim() ?? "";
  return line.length > 120 ? line.slice(0, 117) + "…" : line;
}
