// Bring-your-own AI: the providers Marrow knows how to talk to, and a small
// client that streams an answer. Nothing is sent anywhere until the person
// adds a provider in Settings → AI, and then only to that provider.

import { api, onAiEvent, type AiConfig, type AiEvent, type AiMessage, type AiProvider } from "./api";

export interface Preset {
  id: string;
  name: string;
  group: "Popular" | "Chinese" | "On this computer" | "Other";
  protocol: "openai" | "anthropic";
  baseUrl: string;
  /** Local servers don't need one. */
  needsKey: boolean;
  /** Where to get a key. */
  keyUrl?: string;
  /** Shown under the address (regions and such). */
  hint?: string;
}

export const PRESETS: Preset[] = [
  { id: "openai", name: "OpenAI (ChatGPT)", group: "Popular", protocol: "openai", baseUrl: "https://api.openai.com/v1", needsKey: true, keyUrl: "https://platform.openai.com/api-keys" },
  { id: "anthropic", name: "Anthropic Claude", group: "Popular", protocol: "anthropic", baseUrl: "https://api.anthropic.com/v1", needsKey: true, keyUrl: "https://console.anthropic.com/settings/keys" },
  { id: "gemini", name: "Google Gemini", group: "Popular", protocol: "openai", baseUrl: "https://generativelanguage.googleapis.com/v1beta/openai", needsKey: true, keyUrl: "https://aistudio.google.com/apikey" },
  { id: "mistral", name: "Mistral", group: "Popular", protocol: "openai", baseUrl: "https://api.mistral.ai/v1", needsKey: true, keyUrl: "https://console.mistral.ai/api-keys" },
  { id: "xai", name: "xAI Grok", group: "Popular", protocol: "openai", baseUrl: "https://api.x.ai/v1", needsKey: true, keyUrl: "https://console.x.ai" },
  { id: "groq", name: "Groq", group: "Popular", protocol: "openai", baseUrl: "https://api.groq.com/openai/v1", needsKey: true, keyUrl: "https://console.groq.com/keys" },
  { id: "openrouter", name: "OpenRouter (many models)", group: "Popular", protocol: "openai", baseUrl: "https://openrouter.ai/api/v1", needsKey: true, keyUrl: "https://openrouter.ai/keys" },
  { id: "perplexity", name: "Perplexity", group: "Popular", protocol: "openai", baseUrl: "https://api.perplexity.ai", needsKey: true, keyUrl: "https://www.perplexity.ai/settings/api" },
  { id: "deepseek", name: "DeepSeek", group: "Chinese", protocol: "openai", baseUrl: "https://api.deepseek.com/v1", needsKey: true, keyUrl: "https://platform.deepseek.com/api_keys" },
  { id: "qwen", name: "Qwen (Alibaba Cloud)", group: "Chinese", protocol: "openai", baseUrl: "https://dashscope-intl.aliyuncs.com/compatible-mode/v1", needsKey: true, keyUrl: "https://bailian.console.alibabacloud.com/", hint: "In mainland China use https://dashscope.aliyuncs.com/compatible-mode/v1" },
  { id: "kimi", name: "Kimi (Moonshot AI)", group: "Chinese", protocol: "openai", baseUrl: "https://api.moonshot.ai/v1", needsKey: true, keyUrl: "https://platform.moonshot.ai/console/api-keys", hint: "In mainland China use https://api.moonshot.cn/v1" },
  { id: "glm", name: "GLM (Zhipu AI)", group: "Chinese", protocol: "openai", baseUrl: "https://open.bigmodel.cn/api/paas/v4", needsKey: true, keyUrl: "https://open.bigmodel.cn/usercenter/apikeys", hint: "Outside China (Z.ai) use https://api.z.ai/api/paas/v4" },
  { id: "ernie", name: "ERNIE (Baidu Qianfan)", group: "Chinese", protocol: "openai", baseUrl: "https://qianfan.baidubce.com/v2", needsKey: true, keyUrl: "https://console.bce.baidu.com/qianfan/" },
  { id: "doubao", name: "Doubao (ByteDance Volcano Engine)", group: "Chinese", protocol: "openai", baseUrl: "https://ark.cn-beijing.volces.com/api/v3", needsKey: true, keyUrl: "https://console.volcengine.com/ark", hint: "The model is usually your endpoint ID (ep-…)." },
  { id: "minimax", name: "MiniMax", group: "Chinese", protocol: "openai", baseUrl: "https://api.minimax.io/v1", needsKey: true, keyUrl: "https://www.minimax.io/platform", hint: "In mainland China use https://api.minimaxi.com/v1" },
  { id: "hunyuan", name: "Hunyuan (Tencent)", group: "Chinese", protocol: "openai", baseUrl: "https://api.hunyuan.cloud.tencent.com/v1", needsKey: true, keyUrl: "https://console.cloud.tencent.com/hunyuan" },
  { id: "ollama", name: "Ollama", group: "On this computer", protocol: "openai", baseUrl: "http://localhost:11434/v1", needsKey: false, keyUrl: "https://ollama.com/download", hint: "Free and private: everything stays on this computer. Start Ollama and pull a model first (e.g. ollama pull llama3.2)." },
  { id: "lmstudio", name: "LM Studio", group: "On this computer", protocol: "openai", baseUrl: "http://localhost:1234/v1", needsKey: false, keyUrl: "https://lmstudio.ai", hint: "Turn on the local server in LM Studio (Developer tab)." },
  { id: "custom", name: "Other (any compatible service)", group: "Other", protocol: "openai", baseUrl: "", needsKey: false, hint: "Any service that works like the OpenAI API (most do) or the Anthropic API. Paste its address, ending in /v1 or similar." },
];

export const preset = (id: string): Preset => PRESETS.find((p) => p.id === id) ?? PRESETS[PRESETS.length - 1];

let config: AiConfig = { providers: [], defaultId: null, embed: null };
const listeners = new Set<() => void>();

export async function loadAiConfig(): Promise<AiConfig> {
  config = await api.aiConfig().catch(() => config);
  for (const fn of listeners) fn();
  return config;
}

export function setAiConfig(c: AiConfig) {
  config = c;
  for (const fn of listeners) fn();
}

export const aiConfig = () => config;
export const onAiConfig = (fn: () => void) => listeners.add(fn);

/** Whether any AI is set up (the AI buttons only show then). */
export const aiReady = () => config.providers.length > 0;

export function defaultProvider(): AiProvider | undefined {
  return config.providers.find((p) => p.id === config.defaultId) ?? config.providers[0];
}

/** True for an AI running on this computer (Ollama, LM Studio): nothing leaves it. */
export const isLocal = (p: AiProvider | undefined) => !!p && /^https?:\/\/(localhost|127\.0\.0\.1|\[::1\])(:|\/)/.test(p.baseUrl);

/** Chat models first: embedding models can't answer questions. */
const isEmbedModel = (m: string) => /embed|bge-|minilm|e5-/i.test(m);
const CHAT_PREFERENCE = ["llama3.2", "llama3.1", "qwen2.5", "qwen3", "gemma3", "gemma2", "mistral", "phi4", "phi3", "llama3"];
const EMBED_PREFERENCE = ["nomic-embed-text", "mxbai-embed-large", "bge-m3", "all-minilm"];

/**
 * Connects Ollama on this computer in one step: picks a chat model (and an
 * embedding model for search by meaning, when one is installed).
 * Returns what's missing, if anything.
 */
export async function setUpOllama(): Promise<{ ok: true; model: string } | { ok: false; reason: "not-running" | "no-model" }> {
  const base = preset("ollama");
  const draft: AiProvider = { id: "", preset: "ollama", name: "Ollama (on this computer)", protocol: base.protocol, baseUrl: base.baseUrl, model: "", hasKey: false };
  let models: string[];
  try {
    models = await api.aiModels(draft);
  } catch {
    return { ok: false, reason: "not-running" };
  }
  const pick = (list: string[], prefs: string[]) => prefs.map((p) => list.find((m) => m.startsWith(p))).find(Boolean) ?? list[0];
  const chat = pick(models.filter((m) => !isEmbedModel(m)), CHAT_PREFERENCE);
  if (!chat) return { ok: false, reason: "no-model" };
  const saved = await api.aiSaveProvider({ ...draft, model: chat });
  const mine = saved.providers.find((p) => p.preset === "ollama" && p.model === chat) ?? saved.providers[saved.providers.length - 1];
  let cfg = await api.aiSetDefault(mine.id);
  const embed = pick(models.filter(isEmbedModel), EMBED_PREFERENCE);
  if (embed) {
    await api.aiSetEmbedding(mine.id, embed);
    cfg = await api.aiConfig();
  }
  setAiConfig(cfg);
  return { ok: true, model: chat };
}

// ----------------------------------------------------------------- running

const handlers = new Map<string, (e: AiEvent) => void>();
let listening: Promise<unknown> | null = null;

export interface AiRun {
  /** The provider's name. */
  provider: Promise<string>;
  /** The whole answer, when it's finished. */
  done: Promise<string>;
  cancel(): void;
}

/** Streams an answer; `onText` gets the text so far. */
export function runAi(system: string, messages: AiMessage[], onText: (text: string) => void, providerId?: string): AiRun {
  listening ??= onAiEvent((e) => handlers.get(e.id)?.(e));
  const id = `ai-${Date.now()}-${Math.random().toString(36).slice(2, 8)}`;
  let text = "";
  let settle!: { ok: (t: string) => void; fail: (e: Error) => void };
  const done = new Promise<string>((ok, fail) => (settle = { ok, fail }));
  handlers.set(id, (e) => {
    if (e.kind === "chunk") {
      text += e.text;
      onText(text);
    } else {
      handlers.delete(id);
      if (e.kind === "done") settle.ok(text);
      else settle.fail(new Error(e.text));
    }
  });
  const provider = (async () => {
    await listening;
    try {
      return await api.aiRun(id, system, messages, providerId);
    } catch (e) {
      handlers.delete(id);
      settle.fail(new Error(String(e)));
      return "";
    }
  })();
  return {
    provider,
    done,
    cancel() {
      void api.aiCancel(id);
      if (handlers.has(id)) {
        handlers.delete(id);
        settle.ok(text);
      }
    },
  };
}
