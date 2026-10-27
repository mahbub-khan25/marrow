//! AI: the person's own providers (OpenAI, Claude, Gemini, Ollama on their
//! computer…), "Ask your notes" with answers that cite the notes they come
//! from, and actions on a note (summarize, translate, find action items).
//!
//! Settings are kept like the desktop app's (`ai.json`); API keys go in the
//! [`SecretStore`](crate::SecretStore). Nothing is sent anywhere until the
//! person sets a provider up and asks something.

use std::io::{BufRead, BufReader};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::files::display_name;
use crate::{fail, render, Marrow, Result};

#[derive(Clone, Debug, Default, Serialize, Deserialize, uniffi::Record)]
#[serde(rename_all = "camelCase", default)]
pub struct AiProvider {
    /// Empty for a new one.
    pub id: String,
    /// The preset it came from ("openai", "anthropic", "ollama"…).
    pub preset: String,
    pub name: String,
    /// "openai" (chat completions) or "anthropic" (messages).
    pub protocol: String,
    pub base_url: String,
    pub model: String,
    /// An API key is saved for it.
    pub has_key: bool,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, uniffi::Record)]
#[serde(rename_all = "camelCase", default)]
pub struct AiConfig {
    pub providers: Vec<AiProvider>,
    pub default_id: Option<String>,
}

#[derive(Clone, Debug, uniffi::Record)]
pub struct AiPreset {
    pub id: String,
    pub name: String,
    pub group: String,
    pub protocol: String,
    pub base_url: String,
    pub needs_key: bool,
    /// Where to get a key.
    pub key_url: String,
    pub hint: String,
}

/// id, name, group, protocol, address, needs a key, where to get one, hint.
type Preset = (&'static str, &'static str, &'static str, &'static str, &'static str, bool, &'static str, &'static str);

/// The same choices the desktop app offers.
const PRESETS: &[Preset] = &[
    ("openai", "OpenAI (ChatGPT)", "Popular", "openai", "https://api.openai.com/v1", true, "https://platform.openai.com/api-keys", ""),
    ("anthropic", "Anthropic Claude", "Popular", "anthropic", "https://api.anthropic.com/v1", true, "https://console.anthropic.com/settings/keys", ""),
    ("gemini", "Google Gemini", "Popular", "openai", "https://generativelanguage.googleapis.com/v1beta/openai", true, "https://aistudio.google.com/apikey", ""),
    ("mistral", "Mistral", "Popular", "openai", "https://api.mistral.ai/v1", true, "https://console.mistral.ai/api-keys", ""),
    ("xai", "xAI Grok", "Popular", "openai", "https://api.x.ai/v1", true, "https://console.x.ai", ""),
    ("groq", "Groq", "Popular", "openai", "https://api.groq.com/openai/v1", true, "https://console.groq.com/keys", ""),
    ("openrouter", "OpenRouter (many models)", "Popular", "openai", "https://openrouter.ai/api/v1", true, "https://openrouter.ai/keys", ""),
    ("perplexity", "Perplexity", "Popular", "openai", "https://api.perplexity.ai", true, "https://www.perplexity.ai/settings/api", ""),
    ("deepseek", "DeepSeek", "Chinese", "openai", "https://api.deepseek.com/v1", true, "https://platform.deepseek.com/api_keys", ""),
    ("qwen", "Qwen (Alibaba Cloud)", "Chinese", "openai", "https://dashscope-intl.aliyuncs.com/compatible-mode/v1", true, "https://bailian.console.alibabacloud.com/", "In mainland China use https://dashscope.aliyuncs.com/compatible-mode/v1"),
    ("kimi", "Kimi (Moonshot AI)", "Chinese", "openai", "https://api.moonshot.ai/v1", true, "https://platform.moonshot.ai/console/api-keys", "In mainland China use https://api.moonshot.cn/v1"),
    ("glm", "GLM (Zhipu AI)", "Chinese", "openai", "https://open.bigmodel.cn/api/paas/v4", true, "https://open.bigmodel.cn/usercenter/apikeys", "Outside China (Z.ai) use https://api.z.ai/api/paas/v4"),
    ("ollama", "Ollama", "On your computer", "openai", "", false, "https://ollama.com/download", "Free and private. Run Ollama on a computer on the same Wi-Fi, started with OLLAMA_HOST=0.0.0.0, and put that computer's address here."),
    ("lmstudio", "LM Studio", "On your computer", "openai", "", false, "https://lmstudio.ai", "Turn on the local server in LM Studio (Developer tab, “Serve on local network”) and put that computer's address here."),
    ("custom", "Other (any compatible service)", "Other", "openai", "", false, "", "Any service that works like the OpenAI API (most do) or the Anthropic API. Paste its address, ending in /v1 or similar."),
];

/// One message of a conversation.
#[derive(Clone, Debug, Serialize, Deserialize, uniffi::Record)]
pub struct AiMessage {
    /// "user" or "assistant".
    pub role: String,
    pub content: String,
}

/// A part of a note an answer drew on.
#[derive(Clone, Debug, uniffi::Record)]
pub struct Source {
    pub path: String,
    pub title: String,
    pub heading: String,
    pub line: u32,
    pub text: String,
}

/// Where an answer arrives, piece by piece. Called from a background thread.
#[uniffi::export(with_foreign)]
pub trait AiListener: Send + Sync {
    /// The parts of notes sent with the question (Ask your notes only).
    fn sources(&self, sources: Vec<Source>);
    /// The answer so far, whole (not just the new part).
    fn answer(&self, text: String);
    fn done(&self, text: String);
    fn failed(&self, message: String);
}

/// A running request; `cancel` stops it.
#[derive(uniffi::Object)]
pub struct AiTask {
    stop: Arc<AtomicBool>,
}

#[uniffi::export]
impl AiTask {
    pub fn cancel(&self) {
        self.stop.store(true, Ordering::SeqCst);
    }
}

/// An action on a note, with its instruction for the AI.
#[derive(Clone, Debug, uniffi::Record)]
pub struct NoteAction {
    pub id: String,
    pub label: String,
    /// The answer replaces the note's text (else it's shown, to copy or add).
    pub replaces: bool,
}

const NOTE_ACTIONS: &[(&str, &str, bool, &str)] = &[
    ("summarize", "Summarize", false, "Summarize this note in a short paragraph."),
    ("key-points", "Key points", false, "List the key points of this note as bullets."),
    ("action-items", "Action items", false, "List the action items and to-dos in this note as Markdown tasks (- [ ] …). If there are none, say so."),
    ("titles", "Suggest a better title", false, "Suggest five good titles for this note. Reply with just the list."),
    ("tags", "Suggest tags", false, "Suggest 3 to 6 tags for this note, as #tags on one line. Reply with just the tags."),
    ("gaps", "Find gaps and questions", false, "What is missing or unclear in this note? List open questions worth answering."),
    ("improve", "Improve writing", true, "Improve the writing of this text: clearer, better flow, more precise. Keep its meaning, language and Markdown formatting."),
    ("fix", "Fix spelling & grammar", true, "Fix the spelling, grammar and punctuation of this text. Change nothing else."),
    ("shorter", "Make shorter", true, "Make this text shorter and more concise. Keep the key points."),
    ("simplify", "Simplify", true, "Rewrite this text in simple, plain words that anyone can understand."),
    ("continue", "Continue writing", false, "Continue writing from where this text ends, in the same style. Reply with only the new text."),
    ("bangla", "Translate to Bangla", true, "Translate this whole note into Bangla. Keep the Markdown formatting."),
    ("english", "Translate to English", true, "Translate this whole note into English. Keep the Markdown formatting."),
];

const SYSTEM: &str = "You are a writing assistant inside Marrow, a Markdown notes app. When asked to write or edit text, reply with only that text, formatted as Markdown: no preamble, no explanation, no surrounding code fence. Keep the language of the text unless asked to translate. When asked a question about a note, answer clearly and briefly in Markdown.";

const VAULT_SYSTEM: &str = "You answer questions using the person's own notes, which are given as numbered excerpts. Use only what the excerpts say. After each sentence that relies on an excerpt, cite it like [1] or [2][3]. If the excerpts don't answer the question, say so in one sentence and suggest what to look for instead. Answer in the language of the question, briefly, in Markdown. Don't add a list of sources at the end; the app shows them.";

/// Long notes are cut so they fit.
const MAX_CONTEXT: usize = 24000;

fn key_id(provider_id: &str) -> String {
    format!("ai:{provider_id}")
}

fn client(timeout: Option<Duration>) -> Result<reqwest::blocking::Client> {
    let mut b = reqwest::blocking::Client::builder().connect_timeout(Duration::from_secs(20)).user_agent(concat!("Marrow/", env!("CARGO_PKG_VERSION")));
    b = b.timeout(timeout);
    b.build().map_err(|e| fail(e.to_string()))
}

fn url(base: &str, path: &str) -> String {
    format!("{}/{}", base.trim().trim_end_matches('/'), path)
}

fn friendly(e: reqwest::Error, p: &AiProvider) -> crate::MarrowError {
    if e.is_connect() {
        if p.preset == "ollama" || p.preset == "lmstudio" {
            return fail(format!("Couldn't reach {} at {}. Is it running, and on the same Wi-Fi?", p.name, p.base_url));
        }
        return fail(format!("Couldn't reach {}. Check the internet connection and the address.", p.name));
    }
    if e.is_timeout() {
        return fail(format!("{} took too long to answer.", p.name));
    }
    fail(e.to_string())
}

fn failure(resp: reqwest::blocking::Response) -> crate::MarrowError {
    let status = resp.status();
    let body = resp.text().unwrap_or_default();
    let msg = serde_json::from_str::<Value>(&body)
        .ok()
        .and_then(|v| {
            v.pointer("/error/message").or_else(|| v.pointer("/error")).or_else(|| v.pointer("/message")).and_then(|m| m.as_str().map(str::to_string))
        })
        .unwrap_or_else(|| body.chars().take(300).collect());
    let hint = match status.as_u16() {
        401 | 403 => " (check the API key)",
        404 => " (check the address and the model name)",
        429 => " (rate limit or no credit left)",
        _ => "",
    };
    fail(format!("{status}{hint}: {msg}"))
}

fn request(c: &reqwest::blocking::Client, p: &AiProvider, key: Option<&str>, system: &str, messages: &[AiMessage], stream: bool) -> reqwest::blocking::RequestBuilder {
    if p.protocol == "anthropic" {
        let mut body = json!({ "model": p.model, "max_tokens": 4096, "messages": messages, "stream": stream });
        if !system.is_empty() {
            body["system"] = json!(system);
        }
        let mut r = c.post(url(&p.base_url, "messages")).header("anthropic-version", "2023-06-01").json(&body);
        if let Some(k) = key {
            r = r.header("x-api-key", k);
        }
        r
    } else {
        let mut all = Vec::with_capacity(messages.len() + 1);
        if !system.is_empty() {
            all.push(json!({ "role": "system", "content": system }));
        }
        all.extend(messages.iter().map(|m| json!({ "role": m.role, "content": m.content })));
        let mut r = c.post(url(&p.base_url, "chat/completions")).json(&json!({ "model": p.model, "messages": all, "stream": stream }));
        if let Some(k) = key {
            r = r.bearer_auth(k);
        }
        r
    }
}

fn delta(protocol: &str, v: &Value) -> Option<String> {
    if protocol == "anthropic" {
        if v.get("type")?.as_str()? == "content_block_delta" {
            return v.pointer("/delta/text")?.as_str().map(str::to_string);
        }
        return None;
    }
    v.pointer("/choices/0/delta/content")?.as_str().map(str::to_string)
}

/// Streams an answer to `listener`, stopping early when `stop` is set.
fn stream(p: &AiProvider, key: Option<String>, system: &str, messages: &[AiMessage], stop: &AtomicBool, listener: &dyn AiListener) -> Result<String> {
    let c = client(None)?;
    let resp = request(&c, p, key.as_deref(), system, messages, true).send().map_err(|e| friendly(e, p))?;
    if !resp.status().is_success() {
        return Err(failure(resp));
    }
    let mut answer = String::new();
    for line in BufReader::new(resp).lines() {
        if stop.load(Ordering::SeqCst) {
            break;
        }
        let line = line.map_err(|e| fail(e.to_string()))?;
        let Some(data) = line.trim().strip_prefix("data:") else { continue };
        let data = data.trim();
        if data == "[DONE]" {
            break;
        }
        let Ok(v) = serde_json::from_str::<Value>(data) else { continue };
        if let Some(e) = v.get("error") {
            return Err(fail(e.get("message").and_then(|m| m.as_str()).map(str::to_string).unwrap_or_else(|| e.to_string())));
        }
        if let Some(t) = delta(&p.protocol, &v) {
            if !t.is_empty() {
                answer.push_str(&t);
                listener.answer(answer.clone());
            }
        }
    }
    Ok(answer)
}

/// Strips a code fence some models wrap the whole answer in.
fn clean_answer(s: &str) -> String {
    let t = s.trim();
    if let Some(rest) = t.strip_prefix("```") {
        if let Some(body) = rest.split_once('\n').map(|(_, b)| b) {
            if let Some(inner) = body.trim_end().strip_suffix("```") {
                return inner.trim_end().to_string();
            }
        }
    }
    t.to_string()
}

impl Marrow {
    fn ai_file(&self) -> std::path::PathBuf {
        self.data_dir.join("ai.json")
    }

    fn load_ai(&self) -> AiConfig {
        let mut cfg: AiConfig = std::fs::read(self.ai_file()).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default();
        for p in &mut cfg.providers {
            p.has_key = self.secrets.get(key_id(&p.id)).is_some();
        }
        cfg
    }

    fn save_ai(&self, cfg: &AiConfig) -> Result<()> {
        let mut clean = cfg.clone();
        for p in &mut clean.providers {
            p.has_key = false;
        }
        marrow_core::atomic::write(self.ai_file(), serde_json::to_vec_pretty(&clean).map_err(|e| fail(e.to_string()))?)?;
        Ok(())
    }

    fn key_for(&self, p: &AiProvider, key: Option<String>) -> Option<String> {
        key.map(|k| k.trim().to_string()).filter(|k| !k.is_empty()).or_else(|| (!p.id.is_empty()).then(|| self.secrets.get(key_id(&p.id))).flatten())
    }

    fn pick(&self, provider_id: Option<String>) -> Result<AiProvider> {
        let cfg = self.load_ai();
        let want = provider_id.or(cfg.default_id.clone());
        let p = cfg
            .providers
            .iter()
            .find(|p| Some(&p.id) == want.as_ref())
            .or(cfg.providers.first())
            .cloned()
            .ok_or_else(|| fail("Set up an AI first, in Settings → AI."))?;
        if p.model.trim().is_empty() {
            return Err(fail(format!("{} has no model chosen. Pick one in Settings → AI.", p.name)));
        }
        Ok(p)
    }

    /// Runs a request on a background thread, reporting to `listener`.
    fn run(&self, provider_id: Option<String>, system: String, messages: Vec<AiMessage>, sources: Option<Vec<Source>>, listener: Arc<dyn AiListener>) -> Result<Arc<AiTask>> {
        let p = self.pick(provider_id)?;
        let key = self.secrets.get(key_id(&p.id));
        let stop = Arc::new(AtomicBool::new(false));
        let task = Arc::new(AiTask { stop: stop.clone() });
        std::thread::Builder::new()
            .name("marrow-ai".into())
            .spawn(move || {
                if let Some(s) = sources {
                    listener.sources(s);
                }
                match stream(&p, key, &system, &messages, &stop, listener.as_ref()) {
                    Ok(text) => listener.done(clean_answer(&text)),
                    Err(e) => listener.failed(e.to_string()),
                }
            })
            .map_err(|e| fail(e.to_string()))?;
        Ok(task)
    }
}

#[uniffi::export]
impl Marrow {
    pub fn ai_presets(&self) -> Vec<AiPreset> {
        PRESETS
            .iter()
            .map(|(id, name, group, protocol, base, needs_key, key_url, hint)| AiPreset {
                id: id.to_string(),
                name: name.to_string(),
                group: group.to_string(),
                protocol: protocol.to_string(),
                base_url: base.to_string(),
                needs_key: *needs_key,
                key_url: key_url.to_string(),
                hint: hint.to_string(),
            })
            .collect()
    }

    pub fn ai_config(&self) -> AiConfig {
        self.load_ai()
    }

    /// Whether any AI is set up (AI buttons only show then).
    pub fn ai_ready(&self) -> bool {
        !self.load_ai().providers.is_empty()
    }

    /// Adds or updates a provider (and its key, if given). The first one becomes the default.
    pub fn ai_save_provider(&self, provider: AiProvider, key: Option<String>) -> Result<AiConfig> {
        let mut p = provider;
        if p.name.trim().is_empty() {
            return Err(fail("Give it a name."));
        }
        if p.base_url.trim().is_empty() {
            return Err(fail("Enter the address."));
        }
        let mut cfg = self.load_ai();
        if p.id.is_empty() {
            p.id = format!("p{}", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis()).unwrap_or(0));
        }
        if let Some(k) = key.map(|k| k.trim().to_string()).filter(|k| !k.is_empty()) {
            self.secrets.set(key_id(&p.id), Some(k));
        }
        match cfg.providers.iter_mut().find(|x| x.id == p.id) {
            Some(x) => *x = p.clone(),
            None => cfg.providers.push(p.clone()),
        }
        if cfg.default_id.is_none() {
            cfg.default_id = Some(p.id.clone());
        }
        self.save_ai(&cfg)?;
        Ok(self.load_ai())
    }

    pub fn ai_remove_provider(&self, id: String) -> Result<AiConfig> {
        let mut cfg = self.load_ai();
        cfg.providers.retain(|p| p.id != id);
        self.secrets.set(key_id(&id), None);
        if cfg.default_id.as_deref() == Some(&id) {
            cfg.default_id = cfg.providers.first().map(|p| p.id.clone());
        }
        self.save_ai(&cfg)?;
        Ok(self.load_ai())
    }

    pub fn ai_set_default(&self, id: String) -> Result<AiConfig> {
        let mut cfg = self.load_ai();
        cfg.default_id = Some(id);
        self.save_ai(&cfg)?;
        Ok(self.load_ai())
    }

    /// The models a provider offers. Slow: call off the main thread.
    pub fn ai_models(&self, provider: AiProvider, key: Option<String>) -> Result<Vec<String>> {
        let key = self.key_for(&provider, key);
        let c = client(Some(Duration::from_secs(20)))?;
        let mut req = c.get(url(&provider.base_url, "models"));
        if provider.protocol == "anthropic" {
            req = req.header("anthropic-version", "2023-06-01");
            if let Some(k) = &key {
                req = req.header("x-api-key", k);
            }
        } else if let Some(k) = &key {
            req = req.bearer_auth(k);
        }
        let resp = req.send().map_err(|e| friendly(e, &provider))?;
        if !resp.status().is_success() {
            return Err(failure(resp));
        }
        let v: Value = resp.json().map_err(|e| fail(e.to_string()))?;
        let list = v.get("data").or_else(|| v.get("models")).and_then(|d| d.as_array()).cloned().unwrap_or_default();
        let mut ids: Vec<String> = list.iter().filter_map(|m| m.get("id").or_else(|| m.get("name")).and_then(|x| x.as_str())).map(|s| s.trim_start_matches("models/").to_string()).collect();
        ids.sort();
        ids.dedup();
        Ok(ids)
    }

    /// Sends a tiny request to check the settings work; returns the reply.
    pub fn ai_test(&self, provider: AiProvider, key: Option<String>) -> Result<String> {
        let key = self.key_for(&provider, key);
        if provider.model.trim().is_empty() {
            return Err(fail("Pick a model first."));
        }
        let c = client(Some(Duration::from_secs(90)))?;
        let msgs = [AiMessage { role: "user".into(), content: "Reply with just the word: ready".into() }];
        let resp = request(&c, &provider, key.as_deref(), "", &msgs, false).send().map_err(|e| friendly(e, &provider))?;
        if !resp.status().is_success() {
            return Err(failure(resp));
        }
        let v: Value = resp.json().map_err(|e| fail(e.to_string()))?;
        let text = if provider.protocol == "anthropic" { v.pointer("/content/0/text") } else { v.pointer("/choices/0/message/content") }
            .and_then(|t| t.as_str())
            .unwrap_or("")
            .trim()
            .chars()
            .take(200)
            .collect::<String>();
        Ok(if text.is_empty() { "Connected.".into() } else { text })
    }

    /// The parts of notes that best match a question (by words).
    pub fn recall(&self, query: String, k: u32) -> Result<Vec<Source>> {
        let chunks = self.with(|e| Ok(e.chunks()))?;
        let hits = marrow_core::engine::recall::bm25(&chunks, &query, k.clamp(1, 40) as usize);
        Ok(hits
            .into_iter()
            .map(|(i, _)| {
                let c = &chunks[i];
                Source { path: c.path.to_string(), title: c.name.clone(), heading: c.heading.clone(), line: c.line, text: c.text.clone() }
            })
            .collect())
    }

    /// Asks a question of the whole vault: the best-matching parts of notes
    /// go to the AI, and the answer cites them as [1], [2]…
    pub fn ask_notes(&self, question: String, history: Vec<AiMessage>, provider_id: Option<String>, listener: Arc<dyn AiListener>) -> Result<Arc<AiTask>> {
        let sources = self.recall(question.clone(), 8)?;
        let content = if sources.is_empty() {
            format!("Question: {question}\n\nNo excerpts from the notes matched this question. Say that nothing in the notes seems to cover it.")
        } else {
            let blocks: Vec<String> = sources
                .iter()
                .enumerate()
                .map(|(i, s)| format!("[{}] {}{}\n{}", i + 1, s.title, if s.heading.is_empty() { String::new() } else { format!(" › {}", s.heading) }, s.text.chars().take(2000).collect::<String>()))
                .collect();
            format!("Excerpts from my notes:\n\n{}\n\nQuestion: {question}", blocks.join("\n\n---\n\n"))
        };
        let mut messages = history;
        messages.push(AiMessage { role: "user".into(), content });
        self.run(provider_id, VAULT_SYSTEM.into(), messages, Some(sources), listener)
    }

    /// Actions offered on a note.
    pub fn note_actions(&self) -> Vec<NoteAction> {
        NOTE_ACTIONS.iter().map(|(id, label, replaces, _)| NoteAction { id: id.to_string(), label: label.to_string(), replaces: *replaces }).collect()
    }

    /// Runs an action (`note_actions`) on a note's text, or a free
    /// `instruction` when `action` is empty.
    pub fn run_note_action(&self, action: String, instruction: String, path: String, text: String, provider_id: Option<String>, listener: Arc<dyn AiListener>) -> Result<Arc<AiTask>> {
        let what = NOTE_ACTIONS.iter().find(|a| a.0 == action).map(|a| a.3.to_string()).unwrap_or(instruction);
        if what.trim().is_empty() {
            return Err(fail("Say what the AI should do."));
        }
        let mut body: String = text.chars().take(MAX_CONTEXT).collect();
        if body.len() < text.len() {
            body.push_str("\n\n[…the rest of the note is cut]");
        }
        let content = format!("{what}\n\nThe note “{}”:\n\n{body}", display_name(&path));
        self.run(provider_id, SYSTEM.into(), vec![AiMessage { role: "user".into(), content }], None, listener)
    }

    /// Continues a conversation (a follow-up question about an answer).
    pub fn ai_chat(&self, messages: Vec<AiMessage>, provider_id: Option<String>, listener: Arc<dyn AiListener>) -> Result<Arc<AiTask>> {
        self.run(provider_id, SYSTEM.into(), messages, None, listener)
    }
}

/// An answer as blocks, with [1] citations turned into links to their notes.
#[uniffi::export]
pub fn render_answer(text: String, sources: Vec<Source>) -> Vec<render::Block> {
    let mut blocks = render::render_markdown(clean_answer(&text));
    for b in &mut blocks {
        let mut out = Vec::with_capacity(b.spans.len());
        for s in b.spans.drain(..) {
            if s.link.is_some() || s.code || !s.text.contains('[') {
                out.push(s);
                continue;
            }
            let mut rest = s.text.as_str();
            while let Some(i) = rest.find('[') {
                let tail = &rest[i + 1..];
                let digits: String = tail.chars().take_while(char::is_ascii_digit).collect();
                let n: usize = digits.parse().unwrap_or(0);
                if n > 0 && n <= sources.len() && tail[digits.len()..].starts_with(']') {
                    if i > 0 {
                        out.push(render::Span { text: rest[..i].to_string(), ..s.clone() });
                    }
                    let src = &sources[n - 1];
                    out.push(render::Span { text: digits.clone(), link: Some(render::SpanLink { kind: render::LinkKind::Note, target: format!("{}#L{}", src.path, src.line) }), ..s.clone() });
                    rest = &tail[digits.len() + 1..];
                } else {
                    out.push(render::Span { text: rest[..i + 1].to_string(), ..s.clone() });
                    rest = tail;
                }
            }
            if !rest.is_empty() {
                out.push(render::Span { text: rest.to_string(), ..s.clone() });
            }
        }
        // Join plain neighbours again.
        let mut joined: Vec<render::Span> = Vec::new();
        for s in out {
            match joined.last_mut() {
                Some(l) if l.link.is_none() && s.link.is_none() && l.bold == s.bold && l.italic == s.italic && l.code == s.code && l.strike == s.strike && l.highlight == s.highlight => l.text.push_str(&s.text),
                _ => joined.push(s),
            }
        }
        b.spans = joined;
    }
    blocks
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn citations_become_links() {
        let src = vec![Source { path: "A.md".into(), title: "A".into(), heading: String::new(), line: 3, text: String::new() }];
        let b = render_answer("It is blue [1]. Not [2] or [x].".into(), src);
        let spans = &b[0].spans;
        assert_eq!(spans[0].text, "It is blue ");
        assert_eq!(spans[1].text, "1");
        assert_eq!(spans[1].link.as_ref().unwrap().target, "A.md#L3");
        assert_eq!(spans[2].text, ". Not [2] or [x].");
        assert_eq!(clean_answer("```markdown\nhi\n```"), "hi");
    }

    #[test]
    fn providers_and_recall() {
        let dir = tempfile::tempdir().unwrap();
        let m = crate::tests::vault(dir.path(), &[("Garden.md", "# Garden\nTomatoes need sun and water every day.\n"), ("Car.md", "Oil change in March.")]);
        assert!(!m.ai_ready());
        let p = m.ai_presets().into_iter().find(|p| p.id == "anthropic").unwrap();
        let cfg = m
            .ai_save_provider(AiProvider { preset: p.id, name: p.name, protocol: p.protocol, base_url: p.base_url, model: "claude".into(), ..Default::default() }, Some("sk-1".into()))
            .unwrap();
        assert!(cfg.providers[0].has_key);
        assert_eq!(cfg.default_id.as_deref(), Some(cfg.providers[0].id.as_str()));
        assert!(!std::fs::read_to_string(dir.path().join("data/ai.json")).unwrap().contains("sk-1"));
        let hits = m.recall("tomatoes sun".into(), 3).unwrap();
        assert_eq!(hits[0].title, "Garden");
        let cfg = m.ai_remove_provider(cfg.providers[0].id.clone()).unwrap();
        assert!(cfg.providers.is_empty() && cfg.default_id.is_none());
    }
}
