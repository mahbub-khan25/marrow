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
    try {
      const { svg } = await mermaid.render(`marrow-mermaid-${++count}`, src);
      el.innerHTML = svg;
    } catch (e) {
      showError(el, src, e);
    }
  }
}

function showError(el: HTMLElement, src: string, e: unknown) {
  el.removeAttribute("data-mermaid");
  const pre = document.createElement("pre");
  pre.className = "mermaid-error";
  pre.textContent = `Mermaid couldn't draw this: ${String((e as Error)?.message ?? e).split("\n")[0]}\n\n${src}`;
  el.replaceChildren(pre);
}
