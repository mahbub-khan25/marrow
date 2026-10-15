// The "/" menu: type / at the start of a line (or after a space) to insert
// headings, lists, callouts, tables, templates and more. Everything it
// inserts is ordinary Markdown.

import { snippet, type Completion, type CompletionContext, type CompletionResult } from "@codemirror/autocomplete";
import { syntaxTree } from "@codemirror/language";
import type { EditorView } from "@codemirror/view";

import { aiOnEditor } from "./ai-panel";
import { CALLOUTS } from "./callouts";
import { isoDay } from "./util";

export interface SlashHooks {
  /** Notes in the templates folder. */
  templates(): Promise<{ name: string; path: string }[]>;
  /** A template's text with {{date}}, {{time}} and {{title}} filled in. */
  templateText(path: string): Promise<string>;
  /** Opens the diagram editor on the ```mermaid block starting at `line` (0-based). */
  editDiagram?(line: number): void;
}

interface Item {
  label: string;
  icon: string;
  section: string;
  /** Extra words to match on. */
  keys?: string;
  color?: string;
  /** Snippet text; `${}` marks where the cursor goes. */
  text?: string;
  /** Block items start on their own line. */
  block?: boolean;
  run?: (view: EditorView, from: number, to: number) => void | Promise<void>;
}

const now = () => new Date().toLocaleTimeString(undefined, { hour: "2-digit", minute: "2-digit" });

const ITEMS: Item[] = [
  {
    label: "Ask AI",
    icon: "sparkle",
    section: "AI",
    keys: "ai write assistant chatgpt claude gemini deepseek qwen ollama generate",
    run: (v, f, t) => {
      v.dispatch({ changes: { from: f, to: t } });
      window.setTimeout(() => aiOnEditor(v), 0);
    },
  },
  { label: "Heading 1", icon: "text-h-one", section: "Basic", keys: "h1 title", text: "# ${}", block: true },
  { label: "Heading 2", icon: "text-h-two", section: "Basic", keys: "h2 subtitle", text: "## ${}", block: true },
  { label: "Heading 3", icon: "text-h-three", section: "Basic", keys: "h3", text: "### ${}", block: true },
  { label: "Bulleted list", icon: "list-bullets", section: "Basic", keys: "ul bullet", text: "- ${}", block: true },
  { label: "Numbered list", icon: "list-numbers", section: "Basic", keys: "ol ordered", text: "1. ${}", block: true },
  { label: "To-do", icon: "check-square", section: "Basic", keys: "task checkbox todo", text: "- [ ] ${}", block: true },
  { label: "Toggle", icon: "caret-circle-right", section: "Basic", keys: "collapse details fold", text: "> [!summary]- ${Title}\n> ${}", block: true },
  { label: "Quote", icon: "quotes", section: "Basic", keys: "blockquote", text: "> ${}", block: true },
  { label: "Divider", icon: "minus", section: "Basic", keys: "hr rule line", text: "---\n${}", block: true },
  ...CALLOUTS.map((c) => ({
    label: `${c.label} callout`,
    icon: c.icon,
    color: c.color,
    section: "Callouts",
    keys: `callout box ${c.type}`,
    text: `> [!${c.type}] \${Title}\n> \${}`,
    block: true,
  })),
  { label: "2 columns", icon: "columns", section: "Insert", keys: "layout side by side", text: "```columns\n${}\n\n+++\n\n\n```", block: true },
  { label: "3 columns", icon: "columns-plus-right", section: "Insert", keys: "layout side by side", text: "```columns\n${}\n\n+++\n\n\n+++\n\n\n```", block: true },
  { label: "Database", icon: "database", section: "Insert", keys: "board kanban calendar gallery view folder notion", text: "```database\nview: ${table}\n```", block: true },
  { label: "Diagram", icon: "flow-arrow", section: "Insert", keys: "mermaid flowchart chart graph visual", run: (v, f, t) => insertDiagram(v, f, t) },
  { label: "Table", icon: "table", section: "Insert", keys: "grid", text: "| ${Column} | Column |\n| --- | --- |\n|  |  |", block: true },
  { label: "Code block", icon: "code-block", section: "Insert", keys: "pre snippet", text: "```${}\n\n```", block: true },
  { label: "Math block", icon: "function", section: "Insert", keys: "latex equation", text: "$$\n${}\n$$", block: true },
  { label: "Link to note", icon: "link-simple", section: "Insert", keys: "wikilink page", text: "[[${}" },
  { label: "Embed note or image", icon: "image", section: "Insert", keys: "transclude picture", text: "![[${}" },
  { label: "Today's date", icon: "calendar-blank", section: "Insert", keys: "date now", run: (v, f, t) => insert(v, f, t, isoDay()) },
  { label: "Current time", icon: "clock", section: "Insert", keys: "time now", run: (v, f, t) => insert(v, f, t, now()) },
  { label: "Footnote", icon: "asterisk", section: "Insert", keys: "reference", text: "[^${1}]" },
  {
    label: "Button",
    icon: "cursor-click",
    section: "Insert",
    keys: "action template",
    text: "```button\nname: ${Add entry}\naction: append\ntemplate: \n```",
    block: true,
  },
];

let slashHooks: SlashHooks | null = null;

/** A starter flowchart, then straight into the diagram editor. */
function insertDiagram(view: EditorView, from: number, to: number) {
  const pre = blockPrefix(view, from);
  const text = `${pre}\`\`\`mermaid\nflowchart LR\n    A[Start] --> B[Next step]\n\`\`\`\n`;
  view.dispatch({ changes: { from, to, insert: text }, selection: { anchor: from + text.length } });
  const line = view.state.doc.lineAt(from + pre.length).number - 1;
  window.setTimeout(() => slashHooks?.editDiagram?.(line), 0);
}

function insert(view: EditorView, from: number, to: number, text: string) {
  view.dispatch({ changes: { from, to, insert: text }, selection: { anchor: from + text.length } });
}

/** Block items get their own line when typed after other text. */
function blockPrefix(view: EditorView, from: number): string {
  const line = view.state.doc.lineAt(from);
  return view.state.sliceDoc(line.from, from).trim() ? "\n" : "";
}

function inCode(view: EditorView, pos: number): boolean {
  for (let n: ReturnType<ReturnType<typeof syntaxTree>["resolveInner"]> | null = syntaxTree(view.state).resolveInner(pos, -1); n; n = n.parent) {
    if (/Code|Frontmatter/.test(n.name)) return true;
  }
  return false;
}

export function slashCompletions(hooks: SlashHooks) {
  slashHooks = hooks;
  return async (ctx: CompletionContext): Promise<CompletionResult | null> => {
    const m = ctx.matchBefore(/(?:^|\s)\/[\p{L}\p{N}\- ]{0,24}$/u);
    if (!m) return null;
    const slash = m.from + m.text.indexOf("/");
    // Only "/" at a word start counts ("a/b" and URLs don't), and not inside code.
    if (ctx.view && inCode(ctx.view, slash)) return null;

    const toCompletion = (it: Item, boost: number): Completion =>
      ({
        label: it.label,
        section: it.section,
        boost,
        apply: (v: EditorView, _c: Completion, _from: number, to: number) => {
          if (it.run) return void it.run(v, slash, to);
          const pre = it.block ? blockPrefix(v, slash) : "";
          snippet(pre + (it.text ?? ""))(v, { label: it.label }, slash, to);
        },
        // Read by slashIcon and filterBy.
        icon: it.icon,
        color: it.color,
        filterText: it.keys,
      }) as Completion;

    const options: Completion[] = ITEMS.map((it, i) => toCompletion(it, -i));

    const templates = await hooks.templates().catch(() => []);
    for (const t of templates) {
      options.push({
        label: t.name,
        section: "Templates",
        boost: -100,
        apply: (v, _c, _from, to) => {
          void hooks.templateText(t.path).then((text) => {
            const pre = blockPrefix(v, slash);
            insert(v, slash, to, pre + text);
          });
        },
        ...({ icon: "file-dashed" } as object),
      });
    }
    const shown = filterBy(m.text.slice(m.text.indexOf("/") + 1), options);
    if (shown.length === 0) return null;
    // Our own matching (label and keywords), so CodeMirror shouldn't filter again.
    return { from: slash + 1, options: shown, filter: false };
  };
}

/** Simple, forgiving matching: every typed word must start a word in the label or its keywords. */
function filterBy(query: string, options: Completion[]): Completion[] {
  const q = query.trim().toLowerCase();
  if (!q) return options;
  const words = q.split(/\s+/);
  const scored: [Completion, number][] = [];
  for (const o of options) {
    const hay = `${o.label} ${(o as { filterText?: string }).filterText ?? ""}`.toLowerCase();
    const parts = hay.split(/[\s-]+/);
    if (!words.every((w) => parts.some((p) => p.startsWith(w)) || hay.includes(w))) continue;
    const score = o.label.toLowerCase().startsWith(q) ? 2 : parts.some((p) => p.startsWith(words[0])) ? 1 : 0;
    scored.push([o, score]);
  }
  scored.sort((a, b) => b[1] - a[1]);
  // While typing, one flat list reads better than sections.
  return scored.map(([o], i) => ({ ...o, section: undefined, boost: -i }));
}

/** Draws the icon at the start of each menu row. */
export const slashIcon = {
  render(c: Completion): Node | null {
    const ic = (c as { icon?: string }).icon;
    if (!ic) return null;
    const el = document.createElement("i");
    el.className = `ph-duotone ph-${ic} cm-slash-icon`;
    const color = (c as { color?: string }).color;
    if (color) el.style.color = color;
    return el;
  },
  position: 20,
};
