// The AI panel: pick what to do (or type it), watch the answer arrive, then
// replace, insert or copy it. Opened from the selection bar, "/ai", and the
// command palette. Works on the open note only, and only when asked.

import { EditorSelection } from "@codemirror/state";
import type { EditorView } from "@codemirror/view";

import { aiConfig, aiReady, defaultProvider, loadAiConfig, runAi, type AiRun } from "./ai";
import { api, type AiMessage, type RecallHit } from "./api";
import { renderMarkdown } from "./preview";
import { openSettings } from "./settings";
import { h, icon } from "./util";

/** selection: edit selected text · write: write at the cursor · note: about the whole note */
export type AiMode = "selection" | "write" | "note" | "vault";

export interface NoteTarget {
  title: string;
  text: () => string;
  /** Adds the answer at the end of the note. */
  append: (text: string) => void;
}

interface Action {
  label: string;
  icon: string;
  /** The instruction sent to the AI. */
  prompt?: string;
  /** A second list (tones, languages). */
  more?: Action[];
  /** Answers that add to the text rather than replace it. */
  adds?: boolean;
}

const LANGUAGES = [
  "English",
  "Chinese (Simplified)",
  "Chinese (Traditional)",
  "Spanish",
  "French",
  "German",
  "Arabic",
  "Hindi",
  "Urdu",
  "Bengali",
  "Portuguese",
  "Russian",
  "Japanese",
  "Korean",
  "Turkish",
  "Indonesian",
];

const SELECTION: Action[] = [
  { label: "Improve writing", icon: "magic-wand", prompt: "Improve the writing of this text: clearer, better flow, more precise. Keep its meaning, language and Markdown formatting." },
  { label: "Fix spelling & grammar", icon: "check", prompt: "Fix the spelling, grammar and punctuation of this text. Change nothing else." },
  { label: "Make shorter", icon: "arrows-in-line-horizontal", prompt: "Make this text shorter and more concise. Keep the key points." },
  { label: "Make longer", icon: "arrows-out-line-horizontal", prompt: "Expand this text with more detail, keeping its style and tone." },
  { label: "Simplify", icon: "feather", prompt: "Rewrite this text in simple, plain words that anyone can understand." },
  {
    label: "Change tone",
    icon: "smiley",
    more: ["Professional", "Friendly", "Casual", "Confident", "Polite", "Direct"].map((t) => ({
      label: t,
      icon: "smiley",
      prompt: `Rewrite this text in a more ${t.toLowerCase()} tone. Keep its meaning and language.`,
    })),
  },
  {
    label: "Translate",
    icon: "translate",
    more: LANGUAGES.map((l) => ({ label: l, icon: "translate", prompt: `Translate this text into ${l}. Keep the Markdown formatting.` })),
  },
  { label: "Summarize", icon: "list-dashes", prompt: "Summarize this text in a few sentences.", adds: true },
  { label: "Explain", icon: "question", prompt: "Explain this text simply: what it means and anything worth knowing about it.", adds: true },
  { label: "Turn into a list", icon: "list-bullets", prompt: "Turn this text into a clear bulleted list." },
  { label: "Turn into a table", icon: "table", prompt: "Turn this text into a Markdown table." },
  { label: "Continue writing", icon: "pen-nib", prompt: "Continue writing from where this text ends, in the same style. Reply with only the new text.", adds: true },
];

const WRITE: Action[] = [
  { label: "Continue writing", icon: "pen-nib", prompt: "Continue writing the note from the cursor, in the same style and language. Reply with only the new text." },
  { label: "Brainstorm ideas", icon: "lightbulb", prompt: "Brainstorm a list of ideas that fit here, based on the note." },
  { label: "Write an outline", icon: "list-numbers", prompt: "Write an outline for this note, as Markdown headings and bullet points." },
  { label: "Summarize the note", icon: "list-dashes", prompt: "Write a short summary of the note." },
  { label: "List action items", icon: "check-square", prompt: "List the action items and to-dos in this note as Markdown tasks (- [ ] …)." },
  { label: "Write a table", icon: "table", prompt: "Write a Markdown table that fits here, based on the note." },
];

const NOTE: Action[] = [
  { label: "Summarize", icon: "list-dashes", prompt: "Summarize this note in a short paragraph." },
  { label: "Key points", icon: "list-bullets", prompt: "List the key points of this note as bullets." },
  { label: "Action items", icon: "check-square", prompt: "List the action items and to-dos in this note as Markdown tasks (- [ ] …). If there are none, say so." },
  { label: "Suggest a better title", icon: "text-aa", prompt: "Suggest five good titles for this note. Reply with just the list." },
  { label: "Suggest tags", icon: "hash", prompt: "Suggest 3 to 6 tags for this note, as #tags on one line. Reply with just the tags." },
  { label: "Find gaps and questions", icon: "magnifying-glass", prompt: "What is missing or unclear in this note? List open questions worth answering." },
  { label: "Translate the note", icon: "translate", more: LANGUAGES.map((l) => ({ label: l, icon: "translate", prompt: `Translate this whole note into ${l}. Keep the Markdown formatting.` })) },
];

const SYSTEM = [
  "You are a writing assistant inside Marrow, a Markdown notes app.",
  "When asked to write or edit text, reply with only that text, formatted as Markdown: no preamble, no explanation, no surrounding code fence.",
  "Keep the language of the text unless asked to translate.",
  "When asked a question about a note, answer clearly and briefly in Markdown.",
].join(" ");

/** Long notes are cut so they fit; the part near the cursor is kept. */
const MAX_CONTEXT = 24000;

function clip(text: string, around = 0): string {
  if (text.length <= MAX_CONTEXT) return text;
  const start = Math.max(0, Math.min(text.length - MAX_CONTEXT, around - MAX_CONTEXT / 2));
  return `${start > 0 ? "…" : ""}${text.slice(start, start + MAX_CONTEXT)}…`;
}

/** Removes a code fence the model wrapped the whole answer in. */
export function cleanAnswer(text: string): string {
  const t = text.trim();
  const m = /^```(?:markdown|md)?\s*\n([\s\S]*?)\n```$/.exec(t);
  return m ? m[1] : t;
}

// ------------------------------------------------------------------ panel

let panel: HTMLElement | null = null;
let run: AiRun | null = null;
let chosenProvider: string | undefined;

export function aiPanelOpen() {
  return panel !== null;
}

export function closeAiPanel() {
  run?.cancel();
  run = null;
  panel?.remove();
  panel = null;
  document.documentElement.toggleAttribute("data-modal", false);
  document.removeEventListener("mousedown", outside, true);
}

function outside(e: MouseEvent) {
  if (!panel || panel.contains(e.target as Node)) return;
  // Keep an answer that's arrived (or arriving); close the empty menu.
  if (panel.dataset.state === "menu") closeAiPanel();
}

/** AI on the editor: the selection if there is one, otherwise writing at the cursor. */
export function aiOnEditor(view: EditorView, title = "") {
  const r = view.state.selection.main;
  const mode: AiMode = r.empty ? "write" : "selection";
  // Leave out blank space at the ends (a triple-click takes the line break too).
  const raw = view.state.sliceDoc(r.from, r.to);
  const from = r.from + (raw.length - raw.trimStart().length);
  const to = Math.max(from, r.to - (raw.length - raw.trimEnd().length));
  const selected = view.state.sliceDoc(from, to);
  const coords = view.coordsAtPos(r.empty ? r.head : r.to) ?? view.coordsAtPos(r.from);
  const anchor = coords ? new DOMRect(coords.left, coords.top, 1, coords.bottom - coords.top) : null;

  const context = () => {
    const doc = view.state.doc.toString();
    return mode === "write" ? clip(`${doc.slice(0, from)}⟦cursor⟧${doc.slice(from)}`, from) : clip(doc, from);
  };

  /** Where the selected text is now (the note may have changed meanwhile). */
  const where = (): { from: number; to: number } => {
    if (view.state.sliceDoc(from, to) === selected) return { from, to };
    const at = view.state.doc.toString().indexOf(selected);
    return at >= 0 ? { from: at, to: at + selected.length } : { from: Math.min(from, view.state.doc.length), to: Math.min(from, view.state.doc.length) };
  };

  const put = (text: string, how: "replace" | "below") => {
    const w = where();
    if (how === "replace") {
      view.dispatch({ changes: { from: w.from, to: w.to, insert: text }, selection: EditorSelection.range(w.from, w.from + text.length), userEvent: "input.ai", scrollIntoView: true });
    } else {
      const line = view.state.doc.lineAt(w.to);
      const atEmpty = mode === "write" && !line.text.trim();
      const pos = atEmpty ? line.from : line.to;
      // Keep a blank line between the answer and whatever follows it.
      const next = line.number < view.state.doc.lines ? view.state.doc.line(line.number + 1).text : "";
      const after = next.trim() ? "\n" : "";
      const insert = (atEmpty ? text : `\n\n${text}`) + after;
      view.dispatch({ changes: { from: pos, to: atEmpty ? line.to : pos, insert }, selection: EditorSelection.cursor(pos + insert.length - after.length), userEvent: "input.ai", scrollIntoView: true });
    }
    view.focus();
  };

  open({
    mode,
    anchor,
    actions: mode === "selection" ? SELECTION : WRITE,
    placeholder: mode === "selection" ? "Ask AI to edit this… e.g. make it a polite email" : "Ask AI to write… e.g. a packing list for a weekend trip",
    message: (instruction) =>
      mode === "selection"
        ? `${instruction}\n\nThe text:\n<<<\n${selected}\n>>>\n\nThe whole note, for context only:\n<<<\n${context()}\n>>>`
        : `${title ? `The note "${title}"` : "The note"}, with ⟦cursor⟧ where the new text will go:\n<<<\n${context()}\n>>>\n\n${instruction}`,
    finish: (text, action) =>
      mode === "selection" && !action?.adds
        ? [
            ["Replace", "check", () => put(text, "replace"), true],
            ["Insert below", "arrow-line-down", () => put(text, "below")],
          ]
        : [["Insert", "arrow-line-down", () => put(text, "below"), true]],
    onClose: () => view.focus(),
  });
}

/** AI about a whole note (from the command palette, works in Preview too). */
export function aiOnNote(note: NoteTarget) {
  open({
    mode: "note",
    anchor: null,
    actions: NOTE,
    placeholder: `Ask anything about “${note.title}”…`,
    message: (instruction) => `The note "${note.title}":\n<<<\n${clip(note.text())}\n>>>\n\n${instruction}`,
    finish: (text) => [["Add to the end of the note", "arrow-line-down", () => note.append(text), true]],
  });
}

interface Options {
  mode: AiMode;
  anchor: DOMRect | null;
  actions: Action[];
  placeholder: string;
  message: (instruction: string) => string | Promise<string>;
  /** Instead of SYSTEM. */
  system?: string;
  /** Shown under the box when there are no actions. */
  hint?: string;
  /** Panel width (default 460). */
  width?: number;
  /** After each redraw of the answer (e.g. to make [1] citations clickable). */
  decorate?: (out: HTMLElement) => void;
  /** Shown under the answer once the question is sent (e.g. sources). */
  below?: () => HTMLElement | null;
  /** Buttons for a finished answer: [label, icon, run, primary?]. */
  finish: (text: string, action: Action | null) => [string, string, () => void, boolean?][];
  onClose?: () => void;
}

function open(o: Options) {
  closeAiPanel();
  const el = h("div.ai-panel", { role: "dialog", "aria-label": "AI", tabindex: "-1" });
  panel = el;
  document.body.append(el);
  document.documentElement.toggleAttribute("data-modal", true);
  document.addEventListener("mousedown", outside, true);
  el.addEventListener("keydown", (e) => {
    if (e.key === "Escape") {
      e.preventDefault();
      e.stopPropagation();
      closeAiPanel();
      o.onClose?.();
    }
  });

  const place = () => {
    const w = Math.min(o.width ?? 460, window.innerWidth - 24);
    el.style.width = `${w}px`;
    const a = o.anchor;
    if (!a) {
      el.style.left = `${Math.round((window.innerWidth - w) / 2)}px`;
      el.style.top = `${Math.round(window.innerHeight * 0.14)}px`;
      return;
    }
    const left = Math.max(12, Math.min(a.left - 20, window.innerWidth - w - 12));
    el.style.left = `${left}px`;
    const below = a.bottom + 10;
    const height = el.offsetHeight || 360;
    el.style.top = `${below + height < window.innerHeight - 12 || a.top < height + 20 ? Math.min(below, window.innerHeight - height - 12) : a.top - height - 10}px`;
  };

  if (!aiReady()) {
    el.dataset.state = "setup";
    el.append(
      header(null),
      h(
        "div.ai-setup",
        null,
        h("p", null, "Connect an AI first. Use your own key for ChatGPT, Claude, Gemini, DeepSeek, Qwen, Kimi and others, or a free model on this computer with Ollama."),
        h(
          "button.btn",
          {
            onclick: () => {
              closeAiPanel();
              openSettings("ai");
            },
          },
          icon("plus"),
          "Set up AI…",
        ),
      ),
    );
    place();
    (el.querySelector("button.btn") as HTMLElement).focus();
    return;
  }

  // ---- menu
  const prompt = h("input.ai-prompt", { placeholder: o.placeholder, spellcheck: true, "aria-label": "Ask AI" }) as HTMLInputElement;
  const list = h("div.ai-actions", { role: "menu" });
  const showActions = (actions: Action[], back?: () => void) => {
    list.replaceChildren(
      ...(back ? [h("button.ai-act.back", { role: "menuitem", onclick: back }, icon("caret-left"), "Back")] : []),
      ...actions.map((a) =>
        h(
          "button.ai-act",
          {
            role: "menuitem",
            onclick: () => (a.more ? showActions(a.more, () => showActions(o.actions)) : start(a.prompt!, a)),
          },
          icon(a.icon),
          h("span", null, a.label),
          a.more ? icon("caret-right") : null,
        ),
      ),
    );
    place();
  };
  prompt.addEventListener("keydown", (e) => {
    if (e.key === "Enter" && prompt.value.trim()) {
      e.preventDefault();
      start(prompt.value.trim(), null);
    } else if (e.key === "ArrowDown") {
      e.preventDefault();
      (list.querySelector(".ai-act") as HTMLElement | null)?.focus();
    }
  });
  list.addEventListener("keydown", (e) => {
    const items = [...list.querySelectorAll<HTMLElement>(".ai-act")];
    const i = items.indexOf(document.activeElement as HTMLElement);
    if (e.key === "ArrowDown" || e.key === "ArrowUp") {
      e.preventDefault();
      const n = i + (e.key === "ArrowDown" ? 1 : -1);
      if (n < 0) prompt.focus();
      else items[Math.min(n, items.length - 1)]?.focus();
    }
  });
  el.dataset.state = "menu";
  el.append(header(null), h("div.ai-ask", null, icon("sparkle"), prompt), list);
  if (o.actions.length) showActions(o.actions);
  else if (o.hint) list.replaceChildren(h("div.ai-hint-line", null, o.hint));
  prompt.focus();

  // ---- answer
  let messages: AiMessage[] = [];
  let action: Action | null = null;
  let answer = "";

  async function start(instruction: string, a: Action | null) {
    action = a;
    const what = a?.label ?? instruction;
    el.dataset.state = "answer";
    el.replaceChildren(header(null), h("div.ai-q", null, icon("sparkle"), h("span", null, what)), h("div.ai-out", null, h("div.ai-wait", null, h("span.ai-dots"), "Looking through your notes…")));
    place();
    let content: string;
    try {
      content = await o.message(instruction);
    } catch (e) {
      el.replaceChildren(header(null), h("div.ai-q", null, icon("sparkle"), h("span", null, what)), h("div.ai-out", null, h("div.ai-error", null, String(e))));
      place();
      return;
    }
    if (!panel || panel !== el) return;
    messages = [{ role: "user", content }];
    ask(what);
  }

  function ask(what: string) {
    el.dataset.state = "answer";
    answer = "";
    const out = h("div.ai-out.preview", { "aria-live": "polite" }, h("div.ai-wait", null, h("span.ai-dots"), "Thinking…"));
    const followUp = h("input.ai-prompt", { placeholder: "Tell AI what to change…", "aria-label": "Follow-up" }) as HTMLInputElement;
    const acts = h("div.ai-buttons");
    const stopBtn = h("button.btn.btn-secondary.ai-stop", { onclick: () => run?.cancel() }, icon("stop"), "Stop");
    acts.append(stopBtn);
    const provider = h("span.ai-provider");
    const below = o.below?.() ?? null;
    el.replaceChildren(header(provider), h("div.ai-q", null, icon("sparkle"), h("span", null, what)), out, ...(below ? [below] : []), acts);
    place();

    let frame = 0;
    const draw = () => {
      frame = 0;
      out.innerHTML = renderMarkdown(cleanAnswer(answer), new Set());
      o.decorate?.(out);
      out.scrollTop = out.scrollHeight;
    };
    run = runAi(
      o.system ?? SYSTEM,
      messages,
      (text) => {
        answer = text;
        if (!frame) frame = requestAnimationFrame(draw);
      },
      chosenProvider,
    );
    const mine = run;
    void mine.provider.then((name) => (provider.textContent = name));
    mine.done.then(
      (text) => {
        if (run !== mine || !panel) return;
        run = null;
        answer = text;
        if (frame) cancelAnimationFrame(frame);
        if (!answer.trim()) {
          out.replaceChildren(h("div.ai-error", null, "The AI sent back an empty answer. Try again, or pick another model in Settings → AI."));
        } else draw();
        const final = cleanAnswer(answer);
        const buttons: HTMLElement[] = [];
        if (final) {
          for (const [label, ic, fn, primary] of o.finish(final, action)) {
            buttons.push(
              h(
                primary ? "button.btn" : "button.btn.btn-secondary",
                {
                  onclick: () => {
                    fn();
                    closeAiPanel();
                  },
                },
                icon(ic),
                label,
              ),
            );
          }
          buttons.push(
            h(
              "button.btn.btn-secondary",
              {
                onclick: (e: Event) => {
                  void navigator.clipboard.writeText(final);
                  const b = e.currentTarget as HTMLElement;
                  b.replaceChildren(icon("check"), "Copied");
                },
              },
              icon("copy"),
              "Copy",
            ),
          );
        }
        buttons.push(
          h("button.icon-btn", { title: "Try again", "aria-label": "Try again", onclick: () => ask(what) }, icon("arrow-clockwise")),
          h(
            "button.icon-btn",
            {
              title: "Discard",
              "aria-label": "Discard",
              onclick: () => {
                closeAiPanel();
                o.onClose?.();
              },
            },
            icon("trash"),
          ),
        );
        acts.replaceChildren(...buttons);
        if (final) {
          followUp.addEventListener("keydown", (e) => {
            if (e.key === "Enter" && followUp.value.trim()) {
              e.preventDefault();
              messages = [...messages, { role: "assistant", content: answer }, { role: "user", content: followUp.value.trim() }];
              ask(followUp.value.trim());
            }
          });
          el.append(h("div.ai-ask.follow", null, icon("chat-circle"), followUp));
        }
        place();
        (acts.querySelector("button") as HTMLElement | null)?.focus();
      },
      (err: Error) => {
        if (run !== mine || !panel) return;
        run = null;
        out.replaceChildren(h("div.ai-error", null, err.message));
        acts.replaceChildren(
          h("button.btn.btn-secondary", { onclick: () => ask(what) }, icon("arrow-clockwise"), "Try again"),
          h(
            "button.btn.btn-secondary",
            {
              onclick: () => {
                closeAiPanel();
                openSettings("ai");
              },
            },
            icon("gear-six"),
            "AI settings",
          ),
        );
        place();
      },
    );
  }

  /** Title row, with a provider switch when there's more than one. */
  function header(provider: HTMLElement | null): HTMLElement {
    const cfg = aiConfig();
    let pick: HTMLElement | null = null;
    if (!provider && cfg.providers.length > 1) {
      const sel = h(
        "select.ai-pick",
        { "aria-label": "AI to use", title: "AI to use" },
        ...cfg.providers.map((p) => h("option", { value: p.id }, p.name)),
      ) as HTMLSelectElement;
      sel.value = chosenProvider && cfg.providers.some((p) => p.id === chosenProvider) ? chosenProvider : (defaultProvider()?.id ?? "");
      sel.addEventListener("change", () => (chosenProvider = sel.value));
      pick = sel;
    } else if (!provider && cfg.providers.length === 1) {
      pick = h("span.ai-provider", null, cfg.providers[0].name);
    }
    return h(
      "div.ai-head",
      null,
      h("span.ai-title", null, icon("sparkle"), "AI"),
      provider ?? pick,
      h(
        "button.icon-btn",
        {
          title: "Close (Esc)",
          "aria-label": "Close",
          onclick: () => {
            closeAiPanel();
            o.onClose?.();
          },
        },
        icon("x"),
      ),
    );
  }
}

// ------------------------------------------------------------ vault Q&A

export interface VaultHooks {
  openNote(path: string, line: number): void;
  /** Saves the question and answer as a new note. */
  saveNote(title: string, text: string): void;
}

const VAULT_SYSTEM = [
  "You answer questions using the person's own notes, which are given as numbered excerpts.",
  "Use only what the excerpts say. After each sentence that relies on an excerpt, cite it like [1] or [2][3].",
  "If the excerpts don't answer the question, say so in one sentence and suggest what to look for instead.",
  "Answer in the language of the question, briefly, in Markdown. Don't add a list of sources at the end; the app shows them.",
].join(" ");

/** Ask a question of the whole vault: the best-matching parts of notes go to the AI, and the answer cites them. */
export function aiOnVault(hooks: VaultHooks) {
  let sources: RecallHit[] = [];
  let note: string | null = null;
  let question = "";
  open({
    mode: "vault",
    anchor: null,
    actions: [],
    width: 600,
    placeholder: "Ask your notes anything…",
    hint: "Marrow finds the parts of your notes that match, sends only those to the AI, and shows where each point comes from.",
    system: VAULT_SYSTEM,
    message: async (q) => {
      question = q;
      const r = await api.aiRecall(q, 8);
      sources = r.hits;
      note = r.note;
      if (!sources.length) {
        return `Question: ${q}\n\nNo excerpts from the notes matched this question. Say that nothing in the notes seems to cover it.`;
      }
      const blocks = sources.map((s, i) => `[${i + 1}] ${s.name}${s.heading ? ` › ${s.heading}` : ""}\n${s.text.slice(0, 2000)}`);
      return `Excerpts from my notes:\n\n${blocks.join("\n\n---\n\n")}\n\nQuestion: ${q}`;
    },
    decorate: (out) => {
      // [1] → a link to that source.
      const walker = document.createTreeWalker(out, NodeFilter.SHOW_TEXT);
      const hits: Text[] = [];
      for (let n = walker.nextNode(); n; n = walker.nextNode()) if (/\[\d+\]/.test(n.textContent ?? "")) hits.push(n as Text);
      for (const t of hits) {
        const frag = document.createDocumentFragment();
        const parts = (t.textContent ?? "").split(/(\[\d+\])/);
        for (const part of parts) {
          const m = /^\[(\d+)\]$/.exec(part);
          const src = m ? sources[Number(m[1]) - 1] : undefined;
          if (src) {
            frag.append(
              h(
                "button.ai-cite",
                { title: `${src.name}${src.heading ? ` › ${src.heading}` : ""}`, onclick: () => hooks.openNote(src.path, src.line) },
                m![1],
              ),
            );
          } else frag.append(part);
        }
        t.replaceWith(frag);
      }
    },
    below: () => {
      const box = h("div.ai-sources");
      if (note) box.append(h("div.ai-note", null, icon("info"), h("span", null, note)));
      if (!sources.length) return box.childNodes.length ? box : null;
      box.append(h("div.ai-sources-head", null, "From your notes"));
      sources.forEach((s, i) =>
        box.append(
          h(
            "button.ai-source",
            { title: `${s.path}, line ${s.line + 1}`, onclick: () => hooks.openNote(s.path, s.line) },
            h("span.n", null, String(i + 1)),
            h("span.t", null, s.name, s.heading ? h("span.h", null, ` › ${s.heading}`) : null),
          ),
        ),
      );
      return box;
    },
    finish: (text) => [
      [
        "Save as a note",
        "note-pencil",
        () => {
          const refs = sources.map((s, i) => `${i + 1}. [[${s.path.replace(/\.md$/i, "")}${s.heading ? `#${s.heading}` : ""}|${s.name}${s.heading ? ` › ${s.heading}` : ""}]]`).join("\n");
          hooks.saveNote(question, `> ${question}\n\n${text}\n\n## Sources\n${refs}\n`);
        },
        true,
      ],
    ],
  });
}

// Settings changes (a provider added or removed) are picked up next time.
void loadAiConfig();
