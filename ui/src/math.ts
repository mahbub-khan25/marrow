// Math in notes, the way Obsidian writes it: `$x^2$` inline and `$$ … $$`
// as a block, rendered with KaTeX. A `$` followed by a space (or a space
// before the closing `$`) isn't math, so prices like "$5 and $10" stay text.

import katex from "katex";
import type MarkdownIt from "markdown-it";
import type StateBlock from "markdown-it/lib/rules_block/state_block.mjs";
import type StateInline from "markdown-it/lib/rules_inline/state_inline.mjs";

export function renderTex(tex: string, display: boolean): string {
  try {
    return katex.renderToString(tex, { displayMode: display, throwOnError: false, output: "html", strict: "ignore" });
  } catch (e) {
    return `<span class="math-error" title="${String(e).replace(/"/g, "&quot;")}">${tex.replace(/</g, "&lt;")}</span>`;
  }
}

function inlineMath(state: StateInline, silent: boolean): boolean {
  const src = state.src;
  const start = state.pos;
  if (src.charCodeAt(start) !== 0x24 /* $ */) return false;
  const display = src.charCodeAt(start + 1) === 0x24;
  const open = display ? 2 : 1;
  const first = src[start + open];
  if (!first || (!display && /\s/.test(first))) return false;
  let end = start + open;
  while (end < src.length) {
    end = src.indexOf(display ? "$$" : "$", end);
    if (end < 0) return false;
    // Skip an escaped dollar.
    if (src[end - 1] === "\\") {
      end++;
      continue;
    }
    if (!display && (/\s/.test(src[end - 1]) || /\d/.test(src[end + 1] ?? ""))) {
      end++;
      continue;
    }
    break;
  }
  if (end <= start + open) return false;
  if (!silent) {
    const t = state.push(display ? "math_inline_display" : "math_inline", "math", 0);
    t.content = src.slice(start + open, end);
    t.markup = display ? "$$" : "$";
  }
  state.pos = end + open;
  return true;
}

function blockMath(state: StateBlock, startLine: number, endLine: number, silent: boolean): boolean {
  let pos = state.bMarks[startLine] + state.tShift[startLine];
  let max = state.eMarks[startLine];
  if (state.sCount[startLine] - state.blkIndent >= 4) return false;
  if (state.src.slice(pos, pos + 2) !== "$$") return false;
  pos += 2;
  let firstLine = state.src.slice(pos, max);
  let content = "";
  let next = startLine;
  let found = false;
  if (firstLine.trim().endsWith("$$")) {
    // $$ … $$ on one line.
    firstLine = firstLine.trim().slice(0, -2);
    content = firstLine;
    found = true;
  } else {
    const lines = [firstLine];
    while (++next < endLine) {
      pos = state.bMarks[next] + state.tShift[next];
      max = state.eMarks[next];
      const line = state.src.slice(state.bMarks[next], max);
      const t = state.src.slice(pos, max).trimEnd();
      if (t.endsWith("$$")) {
        lines.push(line.slice(0, line.lastIndexOf("$$")));
        found = true;
        break;
      }
      lines.push(line);
    }
    content = lines.join("\n");
  }
  if (!found) return false;
  if (silent) return true;
  state.line = next + 1;
  const t = state.push("math_block", "math", 0);
  t.block = true;
  t.content = content.trim();
  t.map = [startLine, state.line];
  t.markup = "$$";
  return true;
}

export function mathPlugin(md: MarkdownIt) {
  md.inline.ruler.after("escape", "math_inline", inlineMath);
  md.block.ruler.before("fence", "math_block", blockMath, { alt: ["paragraph", "reference", "blockquote", "list"] });
  md.renderer.rules.math_inline = (toks, i) => renderTex(toks[i].content, false);
  md.renderer.rules.math_inline_display = (toks, i) => renderTex(toks[i].content, true);
  md.renderer.rules.math_block = (toks, i) => {
    const line = toks[i].attrGet("data-line");
    return `<div class="math-block"${line ? ` data-line="${line}"` : ""}>${renderTex(toks[i].content, true)}</div>\n`;
  };
}
