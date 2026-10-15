// Print a note, save it as PDF (through the print dialog), or export it as a
// single self-contained HTML file that opens anywhere.

import { save } from "@tauri-apps/plugin-dialog";

import { api } from "./api";
import { h } from "./util";

/** Waits until the images in `el` have loaded (or failed), up to a few seconds. */
async function imagesReady(el: HTMLElement) {
  const imgs = Array.from(el.querySelectorAll("img"));
  await Promise.race([
    Promise.all(
      imgs.map((img) =>
        img.complete
          ? Promise.resolve()
          : new Promise<void>((r) => {
              img.addEventListener("load", () => r(), { once: true });
              img.addEventListener("error", () => r(), { once: true });
            }),
      ),
    ),
    new Promise((r) => setTimeout(r, 4000)),
  ]);
}

/** Prints the note. For a PDF, the person picks "Save as PDF" as the printer. */
export async function printNote(title: string, el: HTMLElement) {
  document.getElementById("print-root")?.remove();
  const root = h("div", { id: "print-root" }, el);
  document.body.append(root);
  document.documentElement.toggleAttribute("data-printing", true);
  await imagesReady(el);
  // The PDF's suggested file name comes from the page title.
  const before = document.title;
  document.title = title;
  const done = () => {
    document.title = before;
    document.documentElement.toggleAttribute("data-printing", false);
    root.remove();
  };
  window.addEventListener("afterprint", done, { once: true });
  // Some engines block in print() and don't send afterprint.
  window.print();
  window.setTimeout(() => {
    if (!document.documentElement.hasAttribute("data-printing")) return;
    if (document.hasFocus()) done();
  }, 1500);
}

// --------------------------------------------------------------- HTML file

function toDataUrl(blob: Blob): Promise<string> {
  return new Promise((ok, fail) => {
    const r = new FileReader();
    r.onload = () => ok(String(r.result));
    r.onerror = () => fail(r.error);
    r.readAsDataURL(blob);
  });
}

async function inline(url: string): Promise<string | null> {
  try {
    const res = await fetch(url);
    if (!res.ok) return null;
    return await toDataUrl(await res.blob());
  } catch {
    return null;
  }
}

/** A selector without pseudo-elements and states, to test against the content. */
function baseSelector(sel: string): string {
  return sel
    .replace(/::?(before|after|marker|placeholder|selection|first-letter|first-line|-webkit-[\w-]+)/g, "")
    .replace(/:(hover|focus|focus-visible|focus-within|active|visited)/g, "")
    .trim();
}

/** Keeps the style rules that touch something in the exported note. */
function usedCss(content: HTMLElement): { css: string; fonts: CSSFontFaceRule[] } {
  // A small stand-in document so selectors like ":root .preview h1" can match.
  const doc = document.implementation.createHTMLDocument("x");
  doc.documentElement.setAttribute("data-theme", "light");
  doc.body.append(doc.importNode(content, true));
  const keep: string[] = [];
  const fonts: CSSFontFaceRule[] = [];
  const always = /^(:root|html|body|\*)/;
  const visit = (rules: CSSRuleList, out: string[]) => {
    for (const rule of Array.from(rules)) {
      if (rule instanceof CSSFontFaceRule) fonts.push(rule);
      else if (rule instanceof CSSStyleRule) {
        const parts = rule.selectorText.split(",");
        const hit = parts.some((p) => {
          const b = baseSelector(p);
          if (!b || always.test(b)) return true;
          try {
            return doc.querySelector(b) !== null;
          } catch {
            return false;
          }
        });
        if (hit) out.push(rule.cssText);
      } else if (rule instanceof CSSMediaRule) {
        if (/print/.test(rule.conditionText) && !/screen/.test(rule.conditionText)) continue;
        const inner: string[] = [];
        visit(rule.cssRules, inner);
        if (inner.length) out.push(`@media ${rule.conditionText} {\n${inner.join("\n")}\n}`);
      } else if (rule instanceof CSSKeyframesRule) out.push(rule.cssText);
    }
  };
  for (const sheet of Array.from(document.styleSheets)) {
    try {
      visit(sheet.cssRules, keep);
    } catch {
      /* a sheet we can't read */
    }
  }
  return { css: keep.join("\n"), fonts };
}

/** Whether a unicode-range ("U+0-FF, U+131, U+4??") includes `code`. */
function covers(range: string, code: number): boolean {
  return range.split(",").some((part) => {
    const m = /U\+([0-9a-f?]+)(?:-([0-9a-f]+))?/i.exec(part.trim());
    if (!m) return false;
    const lo = parseInt(m[1].replace(/\?/g, "0"), 16);
    const hi = m[2] ? parseInt(m[2], 16) : parseInt(m[1].replace(/\?/g, "f"), 16);
    return code >= lo && code <= hi;
  });
}

/** The @font-face rules the note uses, with the font files embedded. */
async function usedFonts(fonts: CSSFontFaceRule[], content: HTMLElement): Promise<string> {
  const hasMath = !!content.querySelector(".katex");
  const hasIcons = !!content.querySelector("[class*='ph-']");
  const out: string[] = [];
  for (const f of fonts) {
    const family = f.style.getPropertyValue("font-family").replace(/["']/g, "");
    const range = f.style.getPropertyValue("unicode-range");
    const want =
      (/garamond/i.test(family) && (!range || covers(range, 0x41) || covers(range, 0x101))) || (hasMath && /^KaTeX/i.test(family)) || (hasIcons && /phosphor/i.test(family) && /duotone/i.test(family));
    if (!want) continue;
    const src = f.style.getPropertyValue("src");
    // The first url() of a woff2 (or the first one at all).
    const urls = [...src.matchAll(/url\(["']?([^"')]+)["']?\)\s*format\(["']?(\w+)/g)];
    const pick = urls.find((u) => u[2] === "woff2") ?? urls[0];
    if (!pick) continue;
    const abs = new URL(pick[1], f.parentStyleSheet?.href ?? location.href).href;
    const data = await inline(abs);
    if (!data) continue;
    out.push(f.cssText.replace(/src:[^;]+;/, `src: url("${data}") format("${pick[2]}");`));
  }
  return out.join("\n");
}

/** Builds the stand-alone HTML for a note. */
export async function noteHtml(title: string, el: HTMLElement): Promise<string> {
  await imagesReady(el);
  for (const img of Array.from(el.querySelectorAll("img"))) {
    const src = img.getAttribute("src");
    if (src && !src.startsWith("data:") && !/^https?:\/\/(?!asset\.localhost|localhost|tauri\.localhost)/.test(src)) {
      const data = await inline(img.src);
      if (data) img.setAttribute("src", data);
    }
  }
  const { css, fonts } = usedCss(el);
  const fontCss = await usedFonts(fonts, el);
  const esc = (s: string) => s.replace(/&/g, "&amp;").replace(/</g, "&lt;");
  return `<!doctype html>
<html lang="en" data-theme="light">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<meta name="generator" content="Marrow">
<title>${esc(title)}</title>
<style>
${fontCss}
${css}
body { background: var(--color-bg, #fff); color: var(--color-text, #222); margin: 0; }
.export-page { max-width: 46rem; margin: 0 auto; padding: 48px 24px 96px; }
.export-page .preview { --note-size: 19px; }
</style>
</head>
<body>
<main class="export-page">
${el.outerHTML}
</main>
</body>
</html>
`;
}

/** Asks where to save, and writes the HTML file. Returns the path, or null if cancelled. */
export async function exportHtml(title: string, el: HTMLElement): Promise<string | null> {
  const path = await save({
    title: "Export to HTML",
    defaultPath: `${title.replace(/[\\/:*?"<>|]/g, "-")}.html`,
    filters: [{ name: "Web page", extensions: ["html"] }],
  });
  if (!path) return null;
  await api.writeExport(path, await noteHtml(title, el));
  return path;
}
