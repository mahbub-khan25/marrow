// Present a note as slides, full screen.
//
// Slides are separated by a line with just `---` (as in Obsidian's slides).
// A note without separators is split before each top-level heading instead.
// → / Space / PageDown: next · ← / PageUp: back · Home / End · Esc: stop.

import { getCurrentWindow } from "@tauri-apps/api/window";

import { renderMarkdown } from "./preview";
import { h, icon, stripFrontmatter } from "./util";

/** The note's slides, as Markdown. */
export function splitSlides(text: string): string[] {
  const body = stripFrontmatter(text).text;
  const lines = body.split("\n");
  const parts: string[][] = [[]];
  let fence: string | null = null;
  let seps = 0;
  for (const line of lines) {
    const f = /^\s*(`{3,}|~{3,})/.exec(line);
    if (f) {
      if (fence === null) fence = f[1][0];
      else if (f[1][0] === fence) fence = null;
    }
    if (fence === null && /^\s*---\s*$/.test(line)) {
      seps++;
      parts.push([]);
      continue;
    }
    parts[parts.length - 1].push(line);
  }
  let slides = parts.map((p) => p.join("\n").trim());
  if (seps === 0) {
    // No separators: a slide per top-level heading.
    const level = (n: number) => lines.filter((l) => new RegExp(`^#{${n}}\\s`).test(l)).length;
    const top = level(1) >= 2 ? 1 : level(2) >= 1 ? 2 : 0;
    if (top) {
      slides = [];
      let cur: string[] = [];
      fence = null;
      for (const line of lines) {
        const f = /^\s*(`{3,}|~{3,})/.exec(line);
        if (f) {
          if (fence === null) fence = f[1][0];
          else if (f[1][0] === fence) fence = null;
        }
        if (fence === null && new RegExp(`^#{1,${top}}\\s`).test(line) && cur.some((l) => l.trim())) {
          slides.push(cur.join("\n").trim());
          cur = [];
        }
        cur.push(line);
      }
      slides.push(cur.join("\n").trim());
    }
  }
  slides = slides.filter((s) => s.length > 0);
  return slides.length ? slides : [""];
}

let active: (() => void) | null = null;

/**
 * Shows the slides. `hydrate` draws images, embeds, code and diagrams the way
 * Preview does.
 */
export async function present(title: string, text: string, hydrate: (el: HTMLElement) => Promise<void>) {
  active?.();
  const slides = splitSlides(text);
  let i = 0;
  const content = h("div.present-slide.preview");
  const stage = h("div.present-stage", null, content);
  const count = h("span.present-count");
  const bar = h("div.present-bar", null, h("span"));
  const btn = (ic: string, label: string, run: () => void) => h("button.present-btn", { type: "button", title: label, "aria-label": label, onclick: run }, icon(ic));
  const controls = h(
    "div.present-controls",
    null,
    btn("caret-left", "Back (←)", () => go(i - 1)),
    count,
    btn("caret-right", "Next (→)", () => go(i + 1)),
    btn("x", "Stop (Esc)", () => stop()),
  );
  const root = h("div.present", { role: "dialog", "aria-label": `${title} — slides`, tabindex: -1 }, h("div.present-title", null, title), stage, bar, controls);
  document.body.append(root);
  document.documentElement.toggleAttribute("data-modal", true);
  root.focus();

  const win = getCurrentWindow();
  const wasFull = await win.isFullscreen().catch(() => true);
  if (!wasFull) await win.setFullscreen(true).catch(() => {});

  let idle: number | undefined;
  const wake = () => {
    root.classList.add("is-awake");
    window.clearTimeout(idle);
    idle = window.setTimeout(() => root.classList.remove("is-awake"), 1800);
  };
  root.addEventListener("pointermove", wake);

  async function go(n: number) {
    i = Math.max(0, Math.min(slides.length - 1, n));
    content.style.transform = "";
    content.innerHTML = renderMarkdown(slides[i], new Set(), 0);
    for (const x of content.querySelectorAll("[data-line]")) x.removeAttribute("data-line");
    count.textContent = `${i + 1} / ${slides.length}`;
    (bar.firstElementChild as HTMLElement).style.width = `${((i + 1) / slides.length) * 100}%`;
    // A slide that's only a heading is a title slide.
    root.classList.toggle("is-title", content.children.length === 1 && /^H[12]$/.test(content.firstElementChild?.tagName ?? ""));
    await hydrate(content);
    fit();
    // Images and diagrams arrive a moment later.
    for (const img of content.querySelectorAll("img")) img.addEventListener("load", fit, { once: true });
    window.setTimeout(fit, 400);
  }

  /** Shrinks a slide that doesn't fit. */
  function fit() {
    content.style.transform = "";
    const room = stage.clientHeight;
    const need = content.scrollHeight;
    if (need > room && room > 0) content.style.transform = `scale(${Math.max(0.45, room / need)})`;
  }

  const keys = (e: KeyboardEvent) => {
    const k = e.key;
    if (["ArrowRight", "ArrowDown", "PageDown", " ", "Enter", "n"].includes(k)) void go(i + 1);
    else if (["ArrowLeft", "ArrowUp", "PageUp", "Backspace", "p"].includes(k)) void go(i - 1);
    else if (k === "Home") void go(0);
    else if (k === "End") void go(slides.length - 1);
    else if (k === "Escape") stop();
    else return;
    e.preventDefault();
    e.stopPropagation();
  };
  document.addEventListener("keydown", keys, true);
  const onResize = () => fit();
  window.addEventListener("resize", onResize);
  stage.addEventListener("click", (e) => {
    if ((e.target as HTMLElement).closest("a, button, summary, .check")) return;
    void go(e.clientX < window.innerWidth / 3 ? i - 1 : i + 1);
  });

  function stop() {
    document.removeEventListener("keydown", keys, true);
    window.removeEventListener("resize", onResize);
    window.clearTimeout(idle);
    root.remove();
    document.documentElement.toggleAttribute("data-modal", false);
    if (!wasFull) void win.setFullscreen(false).catch(() => {});
    active = null;
  }
  active = stop;
  wake();
  await go(0);
}
