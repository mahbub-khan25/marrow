// AI columns in a database: an instruction ("Summarize in one sentence")
// that the AI answers for each note, saved as a property of that note.

import { aiReady, runAi } from "./ai";
import { api, type NoteRow } from "./api";
import { editNote } from "./database";
import { findDoc } from "./docs";
import { setProp } from "./frontmatter";
import { openSettings } from "./settings";
import { h, icon } from "./util";

const PRESETS: [string, string][] = [
  ["summary", "Summarize the note in one sentence."],
  ["category", "Give the note's category in one or two words."],
  ["next step", "What is the single next action this note implies? Reply with a short imperative phrase, or 'none'."],
  ["sentiment", "Is the note's tone positive, neutral or negative? Reply with one of those words."],
  ["keywords", "Give three keywords for the note, comma-separated."],
];

const SYSTEM =
  "You fill one cell of a table about a note. Reply with only the value: a short line of plain text, no quotes, no Markdown, no explanation. Use the note's language.";

/** Asks the AI for one note's value. */
async function valueFor(row: NoteRow, instruction: string): Promise<string> {
  const text = findDoc(row.path)?.text ?? (await api.readNote(row.path));
  const body = text.length > 8000 ? text.slice(0, 8000) + "…" : text;
  const answer = await runAi(SYSTEM, [{ role: "user", content: `The note "${row.title}":\n<<<\n${body}\n>>>\n\n${instruction}` }], () => {}).done;
  return answer
    .replace(/^```\w*\n?|```$/g, "")
    .split("\n")
    .map((l) => l.trim().replace(/^[-*+]\s+/, ""))
    .filter(Boolean)
    .join(" ")
    .replace(/\*\*|__|`/g, "")
    .replace(/^["“']|["”']$/g, "")
    .slice(0, 300);
}

export interface AiColumnOptions {
  rows: NoteRow[];
  /** Existing AI columns: name → instruction. */
  existing: Record<string, string>;
  /** Adds the column to the database (and remembers the instruction). */
  save(key: string, instruction: string): void;
}

/** The dialog: pick or write an instruction, then fill the column. */
export function openAiColumn(o: AiColumnOptions) {
  if (!aiReady()) {
    openSettings("ai");
    return;
  }
  const name = h("input.input", { placeholder: "Column name, e.g. summary", spellcheck: false }) as HTMLInputElement;
  const instr = h("textarea.input.aic-instr", { rows: 2, placeholder: "What should the AI write for each note? e.g. Summarize the note in one sentence." }) as HTMLTextAreaElement;
  const onlyEmpty = h("input", { type: "checkbox", checked: true }) as HTMLInputElement;
  const status = h("div.aic-status");
  const presets = h(
    "div.aic-presets",
    null,
    ...PRESETS.map(([k, t]) =>
      h(
        "button.aic-chip",
        {
          type: "button",
          onclick: () => {
            name.value = k;
            instr.value = t;
          },
        },
        k,
      ),
    ),
  );
  const existing = Object.entries(o.existing);
  let stop = false;
  let running = false;
  const go = h("button.btn.btn-primary", { onclick: () => void fill() }, icon("sparkle"), "Fill the column");
  const cancel = h("button.btn.btn-secondary", { onclick: () => (running ? (stop = true) : close()) }, "Cancel");
  const card = h(
    "div.dlg.aic",
    { role: "dialog", "aria-label": "AI column" },
    h("h2", null, "AI column"),
    h("p.dlg-lede", null, `The AI reads each of the ${o.rows.length} ${o.rows.length === 1 ? "note" : "notes"} here and writes a value into a property, which shows as a column. The notes' text is sent to the AI you use.`),
    existing.length
      ? h(
          "div.aic-existing",
          null,
          h("div.l", null, "Refill a column"),
          ...existing.map(([k, t]) =>
            h(
              "button.aic-chip.is-existing",
              {
                type: "button",
                title: t,
                onclick: () => {
                  name.value = k;
                  instr.value = t;
                },
              },
              icon("sparkle"),
              k,
            ),
          ),
        )
      : null,
    h("div.field", null, h("label", null, "Start from"), presets),
    h("div.field", null, h("label", null, "Column"), name),
    h("div.field", null, h("label", null, "Instruction"), instr),
    h("label.aic-only", null, onlyEmpty, " Only notes where it's empty"),
    status,
    h("div.dlg-foot", null, cancel, go),
  );
  const wrap = h("div.backdrop.dlg-wrap", null, card);
  document.body.append(wrap);
  document.documentElement.toggleAttribute("data-modal", true);
  name.focus();
  const keys = (e: KeyboardEvent) => {
    if (e.key === "Escape" && !running) {
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

  async function fill() {
    const key = name.value.trim().replace(/\s+/g, " ");
    const instruction = instr.value.trim();
    if (!key || !instruction) {
      status.textContent = "Give the column a name and an instruction.";
      return;
    }
    if (/^(title|name|path|icon|cover|tags)$/i.test(key)) {
      status.textContent = `“${key}” is used by Marrow; pick another name.`;
      return;
    }
    o.save(key, instruction);
    const todo = o.rows.filter((r) => {
      if (!onlyEmpty.checked) return true;
      const v = Object.entries(r.props).find(([k]) => k.toLowerCase() === key.toLowerCase())?.[1];
      return v === undefined || v === null || v === "";
    });
    running = true;
    go.setAttribute("disabled", "");
    name.disabled = instr.disabled = true;
    cancel.textContent = "Stop";
    let done = 0;
    let failed = 0;
    for (const r of todo) {
      if (stop) break;
      status.replaceChildren(h("span.ai-dots"), ` Filling ${done + 1} of ${todo.length}: ${r.title}`);
      try {
        const v = await valueFor(r, instruction);
        if (v) await editNote(r.path, (t) => setProp(t, key, v));
        done++;
      } catch (e) {
        failed++;
        status.textContent = String(e);
        if (failed >= 3 && done === 0) break;
      }
    }
    running = false;
    cancel.textContent = "Close";
    status.textContent = stop
      ? `Stopped after ${done} ${done === 1 ? "note" : "notes"}.`
      : failed
        ? `Filled ${done}; ${failed} couldn't be done. ${status.textContent}`
        : todo.length
          ? `Filled ${done} ${done === 1 ? "note" : "notes"}.`
          : "Every note already has a value. Untick “Only notes where it's empty” to refill.";
    if (!failed && !stop) window.setTimeout(close, 900);
  }
}
