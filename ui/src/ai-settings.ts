// Settings → AI: the providers you've connected, and a form to add one.

import { openUrl } from "@tauri-apps/plugin-opener";

import { aiConfig, loadAiConfig, preset, PRESETS, setAiConfig, type Preset } from "./ai";
import { api, onAiIndex, type AiProvider } from "./api";
import { h, icon } from "./util";

/** The provider being added or edited, if the form is open. */
let editing: { provider: AiProvider; key: string; keyTouched: boolean } | null = null;
let loaded = false;

export function renderAi(body: HTMLElement, rerender: () => void) {
  if (!loaded) {
    loaded = true;
    void loadAiConfig().then(rerender);
  }
  const box = h("div.set-vault.ai-settings");
  body.append(box);
  if (editing) return renderForm(box, rerender);

  const cfg = aiConfig();
  box.append(
    h(
      "div.line",
      null,
      "Use AI to write, edit and summarize your notes, with your own account or a free model on this computer. Text is sent only when you ask, and only to the AI you pick. Keys are saved on this computer, never in your notes.",
    ),
  );
  const list = h("div.ai-list");
  if (!cfg.providers.length) {
    list.append(h("div.ai-empty", null, icon("sparkle"), h("span", null, "No AI connected yet.")));
  }
  for (const p of cfg.providers) {
    const isDefault = p.id === (cfg.defaultId ?? cfg.providers[0]?.id);
    const local = !preset(p.preset).needsKey && /localhost|127\.0\.0\.1/.test(p.baseUrl);
    list.append(
      h(
        "div.ai-item",
        null,
        h("span.ai-item-icon", null, icon(local ? "desktop" : "cloud")),
        h(
          "div.ai-item-text",
          null,
          h("div.l", null, p.name, isDefault && cfg.providers.length > 1 ? h("span.badge", null, "Default") : null),
          h("div.d", null, `${p.model || "No model chosen"} · ${host(p.baseUrl)}${!p.hasKey && preset(p.preset).needsKey ? " · no key" : ""}`),
        ),
        isDefault || cfg.providers.length < 2
          ? null
          : h(
              "button.btn.btn-secondary",
              {
                onclick: async () => {
                  setAiConfig(await api.aiSetDefault(p.id));
                  rerender();
                },
              },
              "Make default",
            ),
        h(
          "button.icon-btn",
          {
            title: "Edit",
            "aria-label": `Edit ${p.name}`,
            onclick: () => {
              editing = { provider: { ...p }, key: "", keyTouched: false };
              rerender();
            },
          },
          icon("pencil-simple"),
        ),
        h(
          "button.icon-btn",
          {
            title: "Remove",
            "aria-label": `Remove ${p.name}`,
            onclick: async () => {
              setAiConfig(await api.aiRemoveProvider(p.id));
              rerender();
            },
          },
          icon("trash"),
        ),
      ),
    );
  }
  box.append(
    list,
    h(
      "div.acts",
      null,
      h(
        "button.btn",
        {
          onclick: () => {
            editing = { provider: fromPreset("openai"), key: "", keyTouched: false };
            rerender();
          },
        },
        icon("plus"),
        "Add AI",
      ),
    ),
    h(
      "div.line.ai-howto",
      null,
      h("b", null, "Using it: "),
      "select text in Source and click ",
      h("b", null, "AI"),
      " on the bar that appears, type ",
      h("b", null, "/ai"),
      " to write at the cursor, or press Ctrl+J. The command palette has “AI: Ask about this note”, and ",
      h("b", null, "Ctrl+Shift+J"),
      " asks all your notes.",
    ),
  );
  if (cfg.providers.length) box.append(meaningSection(rerender));
}

/** Embedding models worth suggesting, by preset. */
const EMBED_MODELS: Record<string, string[]> = {
  openai: ["text-embedding-3-small", "text-embedding-3-large"],
  gemini: ["text-embedding-004", "gemini-embedding-001"],
  mistral: ["mistral-embed"],
  ollama: ["nomic-embed-text", "mxbai-embed-large", "bge-m3"],
  lmstudio: ["text-embedding-nomic-embed-text-v1.5"],
  qwen: ["text-embedding-v3", "text-embedding-v4"],
  glm: ["embedding-3"],
  minimax: ["embo-01"],
  hunyuan: ["hunyuan-embedding"],
  ernie: ["embedding-v1"],
  doubao: [],
  custom: [],
};

let indexProgress: { done: number; total: number } | null = null;
let indexError = "";
let listening = false;

/** Settings → AI → Search by meaning. */
function meaningSection(rerender: () => void): HTMLElement {
  const cfg = aiConfig();
  const usable = cfg.providers.filter((p) => p.protocol !== "anthropic" && p.preset in EMBED_MODELS);
  const current = cfg.embed;
  const sec = h("div.ai-meaning");
  sec.append(
    h("h3", null, "Search by meaning"),
    h(
      "div.line",
      null,
      "Lets “Ask my notes” and search find notes by what they mean, not only by the words in them — “dots in old printing” finds a note about halftone screens. It needs an embedding model; on this computer, Ollama's ",
      h("code", null, "nomic-embed-text"),
      " is free and keeps everything private.",
    ),
  );
  if (!usable.length) {
    sec.append(h("div.ai-hint", null, "None of the AIs you've added offers an embedding model. Add Ollama, OpenAI, Gemini, Mistral or Qwen to use this."));
    return sec;
  }
  const pick = h("select.input", { "aria-label": "Provider for search by meaning" }, h("option", { value: "" }, "Off"), ...usable.map((p) => h("option", { value: p.id }, p.name))) as HTMLSelectElement;
  pick.value = current?.providerId && usable.some((p) => p.id === current.providerId) ? current.providerId : "";
  const list = h("datalist", { id: "embed-models" });
  const model = h("input.input", { list: "embed-models", placeholder: "Embedding model", value: current?.model ?? "", spellcheck: false }) as HTMLInputElement;
  const fillSuggestions = () => {
    const p = usable.find((x) => x.id === pick.value);
    const sug = p ? (EMBED_MODELS[p.preset] ?? []) : [];
    list.replaceChildren(...sug.map((m) => h("option", { value: m })));
    if (p && !model.value && sug.length) model.value = sug[0];
    model.disabled = !p;
  };
  pick.addEventListener("change", () => {
    model.value = "";
    fillSuggestions();
  });
  fillSuggestions();
  const status = h("div.ai-index-status");
  const save = h(
    "button.btn.btn-secondary",
    {
      onclick: async () => {
        await api.aiSetEmbedding(pick.value || null, pick.value ? model.value.trim() : null);
        await loadAiConfig();
        indexError = "";
        rerender();
      },
    },
    "Save",
  );
  sec.append(h("div.ai-row", null, pick, model, list, save), status);

  if (!listening) {
    listening = true;
    void onAiIndex((e) => {
      if (e.error) {
        indexError = e.error;
        indexProgress = null;
      } else if (e.finished) indexProgress = null;
      else if (e.total) indexProgress = { done: e.done ?? 0, total: e.total };
      const el = document.querySelector(".ai-index-status");
      if (el) rerender();
    });
  }

  if (current) {
    void api.aiIndexInfo().then((info) => {
      const pct = info.chunks ? Math.round((info.indexed / info.chunks) * 100) : 0;
      const building = info.building || indexProgress;
      const line = building
        ? `Indexing… ${indexProgress ? `${indexProgress.done} of ${indexProgress.total}` : ""}`
        : info.indexed >= info.chunks
          ? `Ready: all ${info.chunks} parts of your notes are indexed. New and changed notes are added as you ask.`
          : `${info.indexed} of ${info.chunks} parts of your notes indexed (${pct}%).`;
      const parts: (HTMLElement | null)[] = [
        h("div", null, line),
        building ? h("div.ai-meter", null, h("span", { style: `width:${indexProgress ? (indexProgress.done / Math.max(1, indexProgress.total)) * 100 : pct}%` })) : null,
        indexError ? h("div.ai-status", { dataset: { kind: "error" } }, icon("warning-circle"), h("span", null, indexError)) : null,
        !building && info.indexed < info.chunks
          ? h(
              "button.btn",
              {
                onclick: () => {
                  indexError = "";
                  indexProgress = { done: 0, total: info.chunks - info.indexed };
                  void api.aiIndexBuild().catch((e) => (indexError = String(e))).finally(rerender);
                  rerender();
                },
              },
              icon("magic-wand"),
              info.indexed ? "Finish indexing" : "Build the index",
            )
          : null,
      ];
      status.replaceChildren(...parts.filter((x): x is HTMLElement => x !== null));
    });
  } else {
    status.replaceChildren(h("div.ai-hint", null, "Off. Ask my notes still works, matching words."));
  }
  return sec;
}

function fromPreset(id: string): AiProvider {
  const p = preset(id);
  return { id: "", preset: p.id, name: p.id === "custom" ? "" : p.name, protocol: p.protocol, baseUrl: p.baseUrl, model: "", hasKey: false };
}

function host(url: string): string {
  try {
    return new URL(url).host;
  } catch {
    return url;
  }
}

function renderForm(box: HTMLElement, rerender: () => void) {
  const ed = editing!;
  const p = ed.provider;
  const pr: Preset = preset(p.preset);
  const isNew = !p.id;
  const done = () => {
    editing = null;
    rerender();
  };

  // Provider, grouped.
  const groups = [...new Set(PRESETS.map((x) => x.group))];
  const pick = h(
    "select.input",
    { "aria-label": "Provider" },
    ...groups.map((g) => h("optgroup", { label: g }, ...PRESETS.filter((x) => x.group === g).map((x) => h("option", { value: x.id }, x.name)))),
  ) as HTMLSelectElement;
  pick.value = pr.id;
  pick.addEventListener("change", () => {
    const keep = ed.keyTouched ? ed.key : "";
    editing = { provider: { ...fromPreset(pick.value), id: p.id }, key: keep, keyTouched: Boolean(keep) };
    rerender();
  });

  const name = h("input.input", { value: p.name, placeholder: pr.name, spellcheck: false }) as HTMLInputElement;
  name.addEventListener("input", () => (p.name = name.value));

  const key = h("input.input", {
    type: "password",
    value: ed.key,
    placeholder: p.hasKey ? "Saved — type to replace" : pr.needsKey ? "Paste your API key" : "Only if the service needs one",
    autocomplete: "off",
    spellcheck: false,
  }) as HTMLInputElement;
  key.addEventListener("input", () => {
    ed.key = key.value.trim();
    ed.keyTouched = true;
  });
  const showKey = h("button.icon-btn", { type: "button", title: "Show key", "aria-label": "Show key", onclick: () => (key.type = key.type === "password" ? "text" : "password") }, icon("eye"));

  const base = h("input.input", { value: p.baseUrl, placeholder: "https://…/v1", spellcheck: false }) as HTMLInputElement;
  base.addEventListener("input", () => (p.baseUrl = base.value.trim()));

  const models = h("datalist", { id: "ai-models" });
  const model = h("input.input", { value: p.model, list: "ai-models", placeholder: "Pick or type a model", spellcheck: false }) as HTMLInputElement;
  model.addEventListener("input", () => (p.model = model.value.trim()));

  const status = h("div.ai-status", { "aria-live": "polite" });
  const say = (text: string, kind: "ok" | "error" | "busy" = "ok") => {
    status.dataset.kind = kind;
    status.replaceChildren(icon(kind === "ok" ? "check-circle" : kind === "error" ? "warning-circle" : "circle-notch"), h("span", null, text));
  };
  /** The key to send with a test: what's typed, or the saved one. */
  const keyArg = () => (ed.keyTouched ? ed.key : undefined);

  const loadModels = async (quiet = false) => {
    if (!p.baseUrl) return quiet || say("Enter the address first.", "error");
    if (!quiet) say("Asking for the list of models…", "busy");
    try {
      const list = await api.aiModels({ ...p, name: p.name || pr.name }, keyArg());
      models.replaceChildren(...list.map((m) => h("option", { value: m })));
      if (!p.model && list.length) {
        // A sensible first pick for local servers, where any model will do.
        if (!pr.needsKey) {
          p.model = list[0];
          model.value = list[0];
        }
      }
      if (!quiet) say(list.length ? `${list.length} models found. Pick one above.` : "The provider didn't list any models. Type the model's name.", list.length ? "ok" : "error");
    } catch (e) {
      if (!quiet) say(String(e), "error");
    }
  };

  const loadBtn = h("button.btn.btn-secondary", { type: "button", onclick: () => void loadModels() }, icon("list-magnifying-glass"), "Load models");

  const field = (label: string, control: HTMLElement, hint?: Node | string | null) =>
    h("div.field", null, h("label", null, label), control, hint ? h("div.ai-hint", null, hint) : null);

  const keyHint =
    pr.keyUrl && pr.needsKey
      ? h("span", null, "Get a key at ", link(pr.keyUrl, host(pr.keyUrl)), ".")
      : pr.keyUrl && !pr.needsKey
        ? h("span", null, "Download it at ", link(pr.keyUrl, host(pr.keyUrl)), ".")
        : null;

  const protocol =
    pr.id === "custom"
      ? field(
          "API style",
          seg(
            [
              ["openai", "OpenAI-style"],
              ["anthropic", "Anthropic-style"],
            ],
            p.protocol,
            (v) => {
              p.protocol = v as AiProvider["protocol"];
              rerender();
            },
          ),
        )
      : null;

  box.append(
    h("div.ai-form-title", null, h("button.icon-btn", { title: "Back", "aria-label": "Back", onclick: done }, icon("caret-left")), isNew ? "Add AI" : `Edit ${p.name}`),
    field("Provider", pick, pr.group === "On this computer" ? pr.hint : null),
    pr.needsKey || pr.id === "custom" || p.hasKey
      ? field(pr.needsKey ? "API key" : "API key (if the service needs one)", h("div.ai-row", null, key, showKey), keyHint)
      : h("div.ai-hint", null, keyHint),
    field("Model", h("div.ai-row", null, model, loadBtn, models), pr.id === "doubao" ? pr.hint : null),
    h(
      "details.ai-more",
      { open: pr.id === "custom" || undefined },
      h("summary", null, "More options"),
      h(
        "div.ai-more-body",
        null,
        field("Name", name, "How it's shown in Marrow."),
        field("Address", base, pr.group !== "On this computer" && pr.id !== "doubao" ? pr.hint : "Change it if the server runs elsewhere."),
        protocol,
      ),
    ),
    status,
    h(
      "div.acts",
      null,
      h(
        "button.btn",
        {
          onclick: async () => {
            if (!p.baseUrl) return say("Enter the address.", "error");
            if (!p.model) return say("Pick a model (Load models lists them).", "error");
            if (pr.needsKey && !p.hasKey && !ed.key) return say("Paste your API key.", "error");
            try {
              const cfg = await api.aiSaveProvider({ ...p, name: p.name.trim() || pr.name }, keyArg());
              setAiConfig(cfg);
              done();
            } catch (e) {
              say(String(e), "error");
            }
          },
        },
        icon("check"),
        "Save",
      ),
      h(
        "button.btn.btn-secondary",
        {
          onclick: async () => {
            if (!p.model) return say("Pick a model first.", "error");
            say("Sending a short test message…", "busy");
            try {
              const reply = await api.aiTest({ ...p, name: p.name || pr.name }, keyArg());
              say(`It works. ${p.model} replied: “${reply.trim().slice(0, 60)}”`);
            } catch (e) {
              say(String(e), "error");
            }
          },
        },
        icon("plugs-connected"),
        "Test",
      ),
      h("button.btn.btn-secondary", { onclick: done }, "Cancel"),
    ),
  );

  // Local servers need no key, so their models can be listed straight away.
  if (!pr.needsKey && pr.baseUrl && !p.model) void loadModels(true);
  else if (p.hasKey) void loadModels(true);
  (isNew ? pick : model).focus();
}

function link(url: string, text: string): HTMLElement {
  return h("a", { href: "#", onclick: (e: Event) => (e.preventDefault(), void openUrl(url).catch(() => {})) }, text);
}

function seg(opts: [string, string][], value: string, set: (v: string) => void): HTMLElement {
  return h(
    "div.seg",
    { role: "group" },
    ...opts.map(([v, label]) => h("button", { type: "button", "aria-pressed": String(v === value), onclick: () => set(v) }, label)),
  );
}
