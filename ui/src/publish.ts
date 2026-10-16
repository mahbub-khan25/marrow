// Publish a folder of notes as a small static website: one page per note,
// links between them, images, diagrams and maths drawn in advance, a
// sidebar, "Linked from" lists and a search box. Nothing is uploaded: the
// result is a folder that works on any web host (Cloudflare Pages, GitHub
// Pages, Netlify) or opened straight from a USB stick.
//
// Notes are left out when they're locked, or have `publish: false` in their
// properties; with "Only notes marked publish: true" ticked, only those go.

import { open as openDialog } from "@tauri-apps/plugin-dialog";
import { revealItemInDir } from "@tauri-apps/plugin-opener";

import { api, displayName, isNotePath } from "./api";
import { highlightCodeBlocks, renderMermaid } from "./codeblocks";
import { embedRange, rangeText } from "./embeds";
import { usedCss, usedFonts } from "./export";
import { props } from "./frontmatter";
import { linkTarget, renderMarkdown } from "./preview";
import { h, icon, plainText, storage, store, stripFrontmatter } from "./util";

export interface PublishOptions {
  /** Vault folder to publish ("" = the whole vault). */
  folder: string;
  /** Every file in the vault. */
  files: string[];
  vaultRoot: string;
  vaultName: string;
  toast: (text: string, kind?: "info" | "error") => void;
}

interface Page {
  path: string;
  title: string;
  /** Site path, e.g. "projects/plan.html". */
  url: string;
  text: string;
  modified: number | null;
  html?: string;
  plain?: string;
}

const HOME_NAMES = ["index", "home", "readme", "start here", "welcome"];

export function slug(s: string): string {
  return (
    s
      .normalize("NFKD")
      .replace(/[̀-ͯ]/g, "")
      .toLowerCase()
      .replace(/[^\p{L}\p{N}]+/gu, "-")
      .replace(/^-+|-+$/g, "") || "page"
  );
}

/** A relative link from one site page to another file in the site. */
export function relUrl(from: string, to: string): string {
  const up = from.split("/").length - 1;
  return "../".repeat(up) + to.split("/").map(encodeURIComponent).join("/");
}

const esc = (s: string) => s.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;").replace(/"/g, "&quot;");

// ---------------------------------------------------------------- dialog

export function openPublish(o: PublishOptions) {
  const key = `marrow.publish.${o.vaultRoot}::${o.folder}`;
  const saved = storage<{ out?: string; title?: string; onlyMarked?: boolean }>(key, {});
  const folderName = o.folder ? (o.folder.split("/").pop() ?? o.folder) : o.vaultName;
  const title = h("input.input", { value: saved.title ?? folderName, spellcheck: false }) as HTMLInputElement;
  const out = h("input.input", { value: saved.out ?? "", placeholder: "Choose an empty folder, or paste its path", spellcheck: false }) as HTMLInputElement;
  const choose = h("button.btn.btn-secondary", { type: "button", onclick: () => void pick() }, icon("folder-open"), "Choose…");
  const onlyMarked = h("input", { type: "checkbox" }) as HTMLInputElement;
  onlyMarked.checked = !!saved.onlyMarked;
  const status = h("div.dlg-hint");
  const error = h("div.dlg-error");
  const go = h("button.btn.btn-primary", { type: "button", onclick: () => void run() }, icon("globe"), "Publish") as HTMLButtonElement;
  const cancel = h("button.btn.btn-secondary", { type: "button", onclick: () => close() }, "Close");
  const card = h(
    "div.dlg.pub-dlg",
    { role: "dialog", "aria-label": "Publish as a website" },
    h("h2", null, "Publish as a website"),
    h(
      "p.dlg-lede",
      null,
      `Turns ${o.folder ? `the notes in “${folderName}”` : "every note in the vault"} into a small website in a folder of your choice: one page per note, with links, images, diagrams, a sidebar and search. Nothing is uploaded — put the folder on Cloudflare Pages, GitHub Pages or Netlify, or just open it.`,
    ),
    h("div.field", null, h("label", null, "Site title"), title),
    h("div.field", null, h("label", null, "Save the website in"), h("div.pub-row", null, out, choose)),
    h("label.pub-check", null, onlyMarked, " Only notes marked ", h("code", null, "publish: true")),
    h("p.dlg-hint.pub-note", null, "Locked notes, and notes with publish: false in their properties, are always left out."),
    status,
    error,
    h("div.dlg-foot", null, cancel, go),
  );
  const wrap = h("div.backdrop.dlg-wrap", null, card);
  document.body.append(wrap);
  document.documentElement.toggleAttribute("data-modal", true);
  title.focus();
  let busy = false;
  const keys = (e: KeyboardEvent) => {
    if (e.key === "Escape" && !busy) {
      e.stopPropagation();
      close();
    }
  };
  document.addEventListener("keydown", keys, true);

  function close() {
    wrap.remove();
    document.removeEventListener("keydown", keys, true);
    document.documentElement.toggleAttribute("data-modal", false);
  }

  async function pick() {
    const dir = await openDialog({ directory: true, title: "Folder for the website" }).catch(() => null);
    if (typeof dir === "string") out.value = dir;
  }

  async function run() {
    error.textContent = "";
    if (!out.value) {
      await pick();
      if (!out.value) return;
    }
    store(key, { out: out.value, title: title.value.trim(), onlyMarked: onlyMarked.checked });
    busy = true;
    go.disabled = true;
    try {
      const r = await publish({ ...o, out: out.value, title: title.value.trim() || folderName, onlyMarked: onlyMarked.checked }, (msg) => (status.textContent = msg));
      status.replaceChildren(
        h("span", null, `Published ${r.pages} ${r.pages === 1 ? "page" : "pages"}${r.assets ? ` and ${r.assets} ${r.assets === 1 ? "file" : "files"}` : ""}.`),
        r.skipped ? h("span", null, ` Left out ${r.skipped} (locked or not for publishing).`) : "",
        " ",
        h("button.link-btn", { type: "button", onclick: () => void api.siteOpen(out.value).catch((e) => o.toast(String(e), "error")) }, "Open the website"),
        " · ",
        h("button.link-btn", { type: "button", onclick: () => void revealItemInDir(out.value.replace(/[\\/]+$/, "") + (out.value.includes("\\") ? "\\" : "/") + "index.html").catch((e) => o.toast(String(e), "error")) }, "Show the folder"),
      );
      go.replaceChildren(icon("arrows-clockwise"), "Publish again");
    } catch (e) {
      error.textContent = String(e);
      status.textContent = "";
    } finally {
      busy = false;
      go.disabled = false;
    }
  }
}

// ------------------------------------------------------------ publishing

interface Run extends PublishOptions {
  out: string;
  title: string;
  onlyMarked: boolean;
}

export async function publish(o: Run, progress: (msg: string) => void): Promise<{ pages: number; assets: number; skipped: number }> {
  await api.siteBegin(o.out);
  const inFolder = (p: string) => !o.folder || p.startsWith(o.folder + "/");
  const relOf = (p: string) => (o.folder ? p.slice(o.folder.length + 1) : p);

  // 1. Which notes go.
  progress("Reading notes…");
  const pages: Page[] = [];
  let skipped = 0;
  // Locked notes stay out even while they're unlocked for reading in Marrow.
  const locked = await api.lockedPaths();
  const isLocked = (p: string) => locked.notes.includes(p) || locked.folders.some((f) => p.startsWith(f + "/"));
  for (const path of o.files.filter((p) => isNotePath(p) && inFolder(p)).sort()) {
    if (isLocked(path)) {
      skipped++;
      continue;
    }
    let text: string;
    try {
      text = await api.readNote(path);
    } catch {
      skipped++; // unreadable
      continue;
    }
    const p = props(text);
    const flag = p.publish;
    if (flag === false || flag === "false" || (o.onlyMarked && !(flag === true || flag === "true"))) {
      skipped++;
      continue;
    }
    const info = await api.noteInfo(path).catch(() => null);
    pages.push({ path, title: displayName(path), url: "", text, modified: info?.modifiedMs ?? null });
  }
  if (!pages.length) throw new Error("There are no notes to publish here.");

  // 2. Where each one goes. A note called Index/Home/README becomes the home page.
  const taken = new Set<string>(["index.html", "style.css", "site.js", "search.json"]);
  const home = pages.find((p) => !relOf(p.path).includes("/") && HOME_NAMES.includes(p.title.toLowerCase()));
  if (home) home.url = "index.html";
  for (const p of pages) {
    if (p.url) continue;
    const parts = relOf(p.path).replace(/\.md$/i, "").split("/").map(slug);
    let url = parts.join("/") + ".html";
    for (let n = 2; taken.has(url); n++) url = parts.join("/") + `-${n}.html`;
    taken.add(url);
    p.url = url;
  }
  const byPath = new Map(pages.map((p) => [p.path, p]));

  // 3. Render.
  const assets = new Map<string, string>(); // vault path → site path
  const backlinks = new Map<string, Set<string>>(); // page path → linking pages
  const resolveCache = new Map<string, string | null>();
  const resolve = async (target: string, source: string) => {
    const k = `${source}\u0000${target}`;
    if (!resolveCache.has(k)) resolveCache.set(k, target ? await api.resolveLink(target, source).catch(() => null) : source);
    return resolveCache.get(k)!;
  };
  const assetUrl = (vaultPath: string) => {
    let url = assets.get(vaultPath);
    if (!url) {
      const name = vaultPath.split("/").pop()!;
      const dot = name.lastIndexOf(".");
      const base = "assets/" + slug(dot > 0 ? name.slice(0, dot) : name);
      const ext = dot > 0 ? name.slice(dot).toLowerCase() : "";
      url = base + ext;
      for (let n = 2; taken.has(url); n++) url = `${base}-${n}${ext}`;
      taken.add(url);
      assets.set(vaultPath, url);
    }
    return url;
  };
  const all = h("div.preview"); // so ".preview …" rules are kept
  let i = 0;
  for (const page of pages) {
    progress(`Drawing page ${++i} of ${pages.length}: ${page.title}`);
    const el = h("div");
    el.innerHTML = renderMarkdown(page.text, new Set(), 0);
    await finishContent(el, page, 0);
    page.html = el.innerHTML;
    page.plain = plainText(stripFrontmatter(page.text).text).slice(0, 6000);
    all.append(el);
  }

  async function finishContent(el: HTMLElement, page: Page, depth: number) {
    // Headings get ids, so #links work.
    const ids = new Set<string>();
    for (const hd of el.querySelectorAll<HTMLElement>("h1, h2, h3, h4, h5, h6")) {
      let id = slug(hd.textContent ?? "");
      for (let n = 2; ids.has(id); n++) id = `${slug(hd.textContent ?? "")}-${n}`;
      ids.add(id);
      if (depth === 0) hd.id = id;
    }
    for (const a of el.querySelectorAll<HTMLAnchorElement>("a[data-href]")) {
      a.href = a.dataset.href!;
      a.target = "_blank";
      a.rel = "noopener";
      a.removeAttribute("data-href");
    }
    for (const a of el.querySelectorAll<HTMLAnchorElement>("a[data-heading]")) {
      a.href = "#" + slug(a.dataset.heading!);
      a.removeAttribute("data-heading");
    }
    for (const a of el.querySelectorAll<HTMLAnchorElement>("a[data-link]")) {
      const link = a.dataset.link!;
      const target = linkTarget(link);
      const sub = link.includes("#") ? link.slice(link.indexOf("#") + 1) : "";
      const path = await resolve(target, page.path);
      const to = path ? byPath.get(path) : undefined;
      a.removeAttribute("data-link");
      if (to) {
        a.href = (to === page ? "" : relUrl(page.url, to.url)) + (sub && !sub.startsWith("^") ? "#" + slug(sub) : "");
        if (to !== page) {
          if (!backlinks.has(to.path)) backlinks.set(to.path, new Set());
          backlinks.get(to.path)!.add(page.path);
        }
      } else if (path && !isNotePath(path)) {
        a.href = relUrl(page.url, assetUrl(path));
      } else {
        // A note that isn't on the site: keep the words, drop the link.
        a.replaceWith(h("span.site-unpublished", { title: "Not published" }, a.textContent ?? ""));
      }
    }
    for (const img of el.querySelectorAll<HTMLImageElement>("img[data-embed]")) {
      const path = await resolve(img.dataset.embed!, page.path);
      img.removeAttribute("data-embed");
      if (!path || isNotePath(path)) img.remove();
      else {
        img.src = relUrl(page.url, assetUrl(path));
        img.loading = "lazy";
      }
    }
    for (const box of el.querySelectorAll<HTMLElement>("[data-embed-note]")) {
      const link = box.dataset.embedNote!;
      const target = linkTarget(link);
      const sub = link.includes("#") ? link.slice(link.indexOf("#") + 1) : "";
      const path = await resolve(target, page.path);
      const src = path ? byPath.get(path) : undefined;
      const range = src && depth < 1 ? embedRange(src.text, sub) : null;
      if (!src || !range) {
        box.remove();
        continue;
      }
      box.removeAttribute("data-embed-note");
      box.className = "site-embed";
      box.innerHTML = renderMarkdown(rangeText(src.text, range), new Set(), 1);
      await finishContent(box, page, depth + 1);
      box.prepend(h("a.site-embed-from", { href: src === page ? "#" : relUrl(page.url, src.url) }, src.title + (sub && !sub.startsWith("^") ? ` › ${sub}` : "")));
    }
    for (const t of el.querySelectorAll<HTMLElement>(".tag-link")) t.replaceWith(h("span.tag", null, t.textContent ?? ""));
    for (const b of el.querySelectorAll(".md-button")) b.remove();
    for (const db of el.querySelectorAll<HTMLElement>(".db[data-db]")) {
      // A database becomes a plain list of the published notes in its folder.
      const from = /^\s*from:\s*(.+?)\s*$/m.exec(db.dataset.db ?? "")?.[1]?.replace(/^["']|["']$/g, "").replace(/^\/+|\/+$/g, "");
      const dir = from ?? page.path.split("/").slice(0, -1).join("/");
      const rows = pages.filter((p) => p !== page && p.path.split("/").slice(0, -1).join("/") === dir);
      db.replaceWith(
        rows.length
          ? h("ul.site-list", null, ...rows.map((p) => h("li", null, h("a", { href: relUrl(page.url, p.url) }, p.title))))
          : h("p.site-unpublished", null, "(No published notes here.)"),
      );
    }
    for (const c of el.querySelectorAll<HTMLElement>("[data-task]")) {
      c.removeAttribute("data-task");
      c.removeAttribute("role");
    }
    for (const x of el.querySelectorAll("[data-line]")) x.removeAttribute("data-line");
    if (depth === 0) {
      await highlightCodeBlocks(el);
      await renderMermaid(el);
    }
  }

  // 4. Write pages.
  const written: string[] = [];
  const write = async (rel: string, contents: string) => {
    await api.siteWrite(o.out, rel, contents);
    written.push(rel);
  };
  const nav = navTree(pages, relOf);
  if (!home) {
    // A home page listing everything.
    await write(
      "index.html",
      layout(o, { url: "index.html", title: o.title, path: "", text: "", modified: null }, nav, `<p class="site-lede">${pages.length} ${pages.length === 1 ? "page" : "pages"}.</p>${navHtml(nav, "index.html", null, "site-index")}`, null, true),
    );
  }
  i = 0;
  for (const page of pages) {
    progress(`Writing page ${++i} of ${pages.length}…`);
    const back = [...(backlinks.get(page.path) ?? [])].map((p) => byPath.get(p)!).sort((a, b) => a.title.localeCompare(b.title));
    await write(page.url, layout(o, page, nav, page.html ?? "", back, page === home));
  }
  progress("Copying images and files…");
  for (const [vaultPath, url] of assets) {
    try {
      await api.siteCopy(o.out, url, vaultPath);
      written.push(url);
    } catch {
      /* a missing attachment shouldn't stop the site */
    }
  }

  // 5. Style, script, search.
  progress("Writing the style and search…");
  // Heading ids (#welcome…) mustn't pull in the app's own id rules.
  for (const el of all.querySelectorAll("[id]")) el.removeAttribute("id");
  const { css, fonts } = usedCss(all);
  const fontCss = await usedFonts(fonts, all);
  await write("style.css", `/* Made with Marrow */\n${fontCss}\n${css}\n${SITE_CSS}`);
  await write("site.js", SITE_JS);
  await write("search.json", JSON.stringify(pages.map((p) => ({ t: p.title, u: p.url, x: p.plain ?? "" }))));
  await api.siteFinish(o.out, written);
  progress("");
  return { pages: pages.length, assets: assets.size, skipped };
}

// ------------------------------------------------------------ the layout

interface NavNode {
  name: string;
  page?: Page;
  children: NavNode[];
}

function navTree(pages: Page[], relOf: (p: string) => string): NavNode {
  const root: NavNode = { name: "", children: [] };
  for (const p of pages) {
    const parts = relOf(p.path).split("/");
    let node = root;
    for (const dir of parts.slice(0, -1)) {
      let next = node.children.find((c) => !c.page && c.name === dir);
      if (!next) node.children.push((next = { name: dir, children: [] }));
      node = next;
    }
    node.children.push({ name: p.title, page: p, children: [] });
  }
  const sort = (n: NavNode) => {
    n.children.sort((a, b) => Number(!!a.page) - Number(!!b.page) || a.name.localeCompare(b.name, undefined, { numeric: true }));
    n.children.forEach(sort);
  };
  sort(root);
  return root;
}

function navHtml(n: NavNode, from: string, here: Page | null, cls = "site-tree"): string {
  const items = n.children
    .map((c) =>
      c.page
        ? `<li${c.page === here ? ' class="is-here"' : ""}><a href="${esc(relUrl(from, c.page.url))}">${esc(c.name)}</a></li>`
        : `<li class="dir"><span>${esc(c.name)}</span>${navHtml(c, from, here, "")}</li>`,
    )
    .join("");
  return `<ul${cls ? ` class="${cls}"` : ""}>${items}</ul>`;
}

function layout(o: Run, page: Page, nav: NavNode, content: string, back: Page[] | null, isHome: boolean): string {
  const root = relUrl(page.url, "x").slice(0, -1);
  const folder = page.path.split("/").slice(0, -1).join(" / ");
  const date = page.modified ? new Date(page.modified).toLocaleDateString(undefined, { year: "numeric", month: "long", day: "numeric" }) : "";
  const backHtml =
    back && back.length
      ? `<section class="site-back"><h2>Linked from</h2><ul>${back.map((b) => `<li><a href="${esc(relUrl(page.url, b.url))}">${esc(b.title)}</a></li>`).join("")}</ul></section>`
      : "";
  // The note's own first heading is the title when it matches the name.
  const firstH1 = /^\s*<h1[^>]*>(.*?)<\/h1>/i.exec(content);
  const showTitle = !(firstH1 && firstH1[1].replace(/<[^>]+>/g, "").trim().toLowerCase() === page.title.toLowerCase());
  return `<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<meta name="generator" content="Marrow">
<title>${esc(isHome ? o.title : `${page.title} · ${o.title}`)}</title>
<link rel="stylesheet" href="${root}style.css">
<script defer src="${root}site.js"></script>
</head>
<body data-root="${esc(root)}">
<header class="site-head">
<button class="site-menu" type="button" aria-label="Pages">☰</button>
<a class="site-name" href="${root}index.html">${esc(o.title)}</a>
<div class="site-find"><input class="site-search" type="search" placeholder="Search" aria-label="Search"><div class="site-results" hidden></div></div>
</header>
<div class="site-wrap">
<nav class="site-nav" aria-label="Pages">${navHtml(nav, page.url, page)}</nav>
<main class="site-main">
<article class="preview">
${!isHome && folder ? `<div class="site-crumbs">${esc(folder)}</div>` : ""}
${showTitle ? `<h1 class="site-title">${esc(page.title)}</h1>` : ""}
${content}
</article>
${backHtml}
<footer class="site-foot">${date ? `Updated ${esc(date)} · ` : ""}Made with Marrow</footer>
</main>
</div>
</body>
</html>
`;
}

const SITE_CSS = `
/* ---- site layout ---- */
html, body { height: auto; overflow: visible; }
body { margin: 0; background: var(--color-bg); color: var(--color-text); }
.site-head {
  position: sticky; top: 0; z-index: 5; display: flex; align-items: center; gap: 14px;
  padding: 10px 20px; background: color-mix(in srgb, var(--color-bg) 92%, transparent);
  backdrop-filter: blur(8px); border-bottom: 1px solid var(--color-divider, rgba(0,0,0,.1));
}
.site-name { font-family: var(--font-heading, serif); font-weight: 600; font-size: 21px; color: inherit; text-decoration: none; }
.site-menu { display: none; border: 0; background: none; font-size: 20px; cursor: pointer; color: inherit; }
.site-find { position: relative; margin-left: auto; }
.site-search {
  width: 220px; padding: 6px 12px; border-radius: 8px; font: inherit; font-size: 15px; color: inherit;
  border: 1px solid var(--color-divider, rgba(0,0,0,.15)); background: var(--color-surface, #f4f4f4);
}
.site-results {
  position: absolute; right: 0; top: calc(100% + 6px); width: min(420px, 90vw); max-height: 60vh; overflow: auto;
  background: var(--color-bg); border-radius: 10px; box-shadow: 0 10px 30px rgba(0,0,0,.18); padding: 6px;
}
.site-results a { display: block; padding: 8px 10px; border-radius: 6px; color: inherit; text-decoration: none; }
.site-results a:hover, .site-results a.is-on { background: var(--color-surface, #f2f2f2); }
.site-results small { display: block; color: var(--ink-70, #666); font-size: 13px; margin-top: 2px; }
.site-wrap { display: flex; max-width: 1180px; margin: 0 auto; }
.site-nav { flex: none; width: 250px; padding: 28px 18px 40px 24px; position: sticky; top: 56px; align-self: flex-start; max-height: calc(100vh - 56px); overflow: auto; font-size: 15px; }
.site-nav ul, .site-index, .site-index ul { list-style: none; margin: 0; padding: 0; }
.site-nav ul ul, .site-index ul { padding-left: 12px; }
.site-nav li, .site-index li { margin: 2px 0; }
.site-nav a, .site-index a { color: var(--ink-78, #444); text-decoration: none; display: block; padding: 3px 8px; border-radius: 6px; }
.site-nav a:hover, .site-index a:hover { background: var(--color-surface, #f2f2f2); color: var(--color-text); }
.site-nav li.is-here > a { background: var(--color-surface, #eee); color: var(--color-text); font-weight: 600; }
.site-nav .dir > span, .site-index .dir > span { display: block; margin-top: 12px; padding: 2px 8px; font-size: 12px; letter-spacing: .08em; text-transform: uppercase; color: var(--ink-65, #777); }
.site-index { font-size: 18px; }
.site-main { flex: 1; min-width: 0; padding: 36px 32px 60px; }
.site-main .preview { max-width: 46rem; --note-size: 19px; }
.site-crumbs { font-size: 14px; color: var(--ink-65, #777); letter-spacing: .04em; margin-bottom: 4px; }
.site-title { margin-top: 0; }
.site-lede { color: var(--ink-70, #666); }
.site-unpublished { color: var(--ink-70, #777); border-bottom: 1px dotted currentColor; }
.site-embed { margin: 1em 0; padding: 4px 18px 8px; border-left: 3px solid var(--color-accent, #0b8cb5); background: var(--color-surface, #f6f6f6); border-radius: 0 8px 8px 0; }
.site-embed-from { display: block; margin-top: 8px; font-size: 13px; letter-spacing: .04em; color: var(--ink-65, #777); text-decoration: none; }
.site-back { max-width: 46rem; margin-top: 48px; padding-top: 16px; border-top: 1px solid var(--color-divider, rgba(0,0,0,.1)); }
.site-back h2 { font-size: 13px; letter-spacing: .08em; text-transform: uppercase; color: var(--ink-65, #777); margin: 0 0 8px; }
.site-back ul { margin: 0; padding-left: 18px; }
.site-foot { max-width: 46rem; margin-top: 40px; font-size: 13px; color: var(--ink-65, #888); }
.site-main img { max-width: 100%; height: auto; }
@media (max-width: 820px) {
  .site-menu { display: block; }
  .site-nav { display: none; position: fixed; inset: 52px 0 0 0; align-self: stretch; width: auto; max-height: none; background: var(--color-bg); z-index: 4; }
  body.nav-open .site-nav { display: block; }
  .site-main { padding: 24px 18px 48px; }
  .site-search { width: 140px; }
}
`;

const SITE_JS = `// Made with Marrow: the page menu on small screens, and search.
(function () {
  var root = document.body.getAttribute("data-root") || "";
  var menu = document.querySelector(".site-menu");
  if (menu) menu.addEventListener("click", function () { document.body.classList.toggle("nav-open"); });
  var box = document.querySelector(".site-search");
  var out = document.querySelector(".site-results");
  if (!box || !out) return;
  var index = null, loading = null, sel = 0;
  function load() {
    if (index || loading) return loading;
    loading = fetch(root + "search.json").then(function (r) { return r.json(); }).then(function (d) { index = d; }).catch(function () { index = []; });
    return loading;
  }
  function esc(s) { return s.replace(/[&<>"]/g, function (c) { return { "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;" }[c]; }); }
  function show() {
    var q = box.value.trim().toLowerCase();
    if (!q || !index) { out.hidden = true; return; }
    var words = q.split(/\\s+/);
    var hits = index.map(function (p) {
      var t = p.t.toLowerCase(), x = p.x.toLowerCase(), score = 0;
      for (var i = 0; i < words.length; i++) {
        if (t.indexOf(words[i]) >= 0) score += 10;
        else if (x.indexOf(words[i]) >= 0) score += 1;
        else return null;
      }
      var at = x.indexOf(words[0]);
      var snip = at >= 0 ? p.x.slice(Math.max(0, at - 40), at + 90) : p.x.slice(0, 120);
      return { p: p, score: score, snip: snip };
    }).filter(Boolean).sort(function (a, b) { return b.score - a.score; }).slice(0, 12);
    sel = 0;
    out.innerHTML = hits.length
      ? hits.map(function (h, i) { return '<a href="' + root + encodeURI(h.p.u) + '"' + (i === 0 ? ' class="is-on"' : "") + ">" + esc(h.p.t) + "<small>" + esc(h.snip) + "</small></a>"; }).join("")
      : '<a>No pages found</a>';
    out.hidden = false;
  }
  box.addEventListener("focus", function () { load(); });
  box.addEventListener("input", function () { var p = load(); if (p) p.then(show); else show(); });
  box.addEventListener("keydown", function (e) {
    var links = out.querySelectorAll("a[href]");
    if (e.key === "ArrowDown" || e.key === "ArrowUp") {
      e.preventDefault();
      if (!links.length) return;
      links[sel].classList.remove("is-on");
      sel = (sel + (e.key === "ArrowDown" ? 1 : links.length - 1)) % links.length;
      links[sel].classList.add("is-on");
    } else if (e.key === "Enter" && links[sel]) {
      location.href = links[sel].getAttribute("href");
    } else if (e.key === "Escape") {
      out.hidden = true; box.blur();
    }
  });
  document.addEventListener("click", function (e) { if (!e.target.closest(".site-find")) out.hidden = true; });
  document.addEventListener("keydown", function (e) {
    if (e.key === "/" && document.activeElement !== box) { e.preventDefault(); box.focus(); }
  });
})();
`;
