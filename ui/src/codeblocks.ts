// Code blocks in Preview: syntax colouring (with the same language support
// the Source editor uses) and Mermaid diagrams. Both load on first use.

import { LanguageDescription } from "@codemirror/language";
import { languages } from "@codemirror/language-data";
import { classHighlighter, highlightCode } from "@lezer/highlight";

/** Colours `<pre><code class="language-x">` blocks inside `root`. */
export async function highlightCodeBlocks(root: HTMLElement) {
  for (const code of root.querySelectorAll<HTMLElement>("pre > code[class*='language-']")) {
    if (code.dataset.hl) continue;
    code.dataset.hl = "1";
    const name = /language-([\w+#.-]+)/.exec(code.className)?.[1];
    const desc = name ? LanguageDescription.matchLanguageName(languages, name, true) : null;
    if (!desc) continue;
    try {
      const support = await desc.load();
      const text = code.textContent ?? "";
      const tree = support.language.parser.parse(text);
      const out = document.createDocumentFragment();
      highlightCode(
        text,
        tree,
        classHighlighter,
        (t, cls) => {
          if (!cls) out.append(t);
          else {
            const s = document.createElement("span");
            s.className = cls;
            s.textContent = t;
            out.append(s);
          }
        },
        () => out.append("\n"),
      );
      code.replaceChildren(out);
    } catch {
      /* leave it plain */
    }
  }
}

let mermaidLib: Promise<typeof import("mermaid").default> | null = null;
let count = 0;

/** Mermaid, loaded on first use and set up for the current theme. */
export async function loadMermaid(): Promise<typeof import("mermaid").default> {
  mermaidLib ??= import("mermaid").then((m) => m.default);
  const mermaid = await mermaidLib;
  const t = document.documentElement.dataset.theme;
  const dark = t ? t === "dark" : matchMedia("(prefers-color-scheme: dark)").matches;
  mermaid.initialize({
    startOnLoad: false,
    securityLevel: "strict",
    theme: dark ? "dark" : "neutral",
    fontFamily: getComputedStyle(document.documentElement).getPropertyValue("--font-body") || "serif",
  });
  return mermaid;
}

/** Draws ```mermaid blocks (`.mermaid-block[data-mermaid]`) inside `root`. */
export async function renderMermaid(root: HTMLElement) {
  const blocks = [...root.querySelectorAll<HTMLElement>(".mermaid-block[data-mermaid]")];
  if (!blocks.length) return;
  let mermaid: typeof import("mermaid").default;
  try {
    mermaid = await loadMermaid();
  } catch (e) {
    mermaidLib = null;
    for (const el of blocks) showError(el, decodeURIComponent(el.dataset.mermaid ?? ""), e);
    return;
  }
  for (const el of blocks) {
    // Encoded, because the sanitiser drops attributes containing "-->".
    const src = decodeURIComponent(el.dataset.mermaid ?? "");
    el.removeAttribute("data-mermaid");
    // Kept so the diagram can be redrawn when the theme changes.
    drawn.set(el, src);
    try {
      const { svg } = await mermaid.render(`marrow-mermaid-${++count}`, src);
      el.innerHTML = svg;
    } catch (e) {
      showError(el, src, e);
    }
  }
}

/** Diagrams on screen and their source, for redrawing in a new theme. */
const drawn = new WeakMap<HTMLElement, string>();
let lastDark: boolean | null = null;

function isDark() {
  const t = document.documentElement.dataset.theme;
  return t ? t === "dark" : matchMedia("(prefers-color-scheme: dark)").matches;
}

/** Redraws the diagrams on screen after a switch between light and dark. */
function onThemeChange() {
  const dark = isDark();
  if (lastDark === null) lastDark = dark;
  if (dark === lastDark) return;
  lastDark = dark;
  for (const el of document.querySelectorAll<HTMLElement>(".mermaid-block")) {
    const src = drawn.get(el);
    if (src !== undefined) el.dataset.mermaid = encodeURIComponent(src);
  }
  void renderMermaid(document.body);
}
lastDark = isDark();
new MutationObserver(onThemeChange).observe(document.documentElement, { attributes: true, attributeFilter: ["data-theme"] });
matchMedia("(prefers-color-scheme: dark)").addEventListener("change", onThemeChange);

function showError(el: HTMLElement, src: string, e: unknown) {
  el.removeAttribute("data-mermaid");
  const pre = document.createElement("pre");
  pre.className = "mermaid-error";
  pre.textContent = `Mermaid couldn't draw this: ${String((e as Error)?.message ?? e).split("\n")[0]}\n\n${src}`;
  el.replaceChildren(pre);
}
