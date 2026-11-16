//! Bring-your-own AI. Marrow has no AI of its own: the person adds a provider
//! (OpenAI, Claude, Gemini, DeepSeek, Qwen, Kimi, a local Ollama…) with their
//! own key, and Marrow sends what they ask for to that provider only.
//!
//! Almost every provider speaks the OpenAI chat-completions protocol; Claude
//! uses Anthropic's Messages API. Requests are made here, not in the window,
//! so browser rules (CORS) don't get in the way, and answers stream back as
//! `ai-event` events. On a Hyperlume desktop there's a third way, Hyperlume
//! AI: no request over the network from Marrow, but the `hyperlume-ai`
//! program, which uses whatever AI the person chose in Hyperlume.
//!
//! Provider settings live in `ai.json` in the config folder. API keys don't:
//! they go in Windows Credential Manager or the macOS Keychain. In portable
//! mode (and on Linux) they're kept in `ai-keys.json` beside the settings, so
//! they travel with the Marrow folder.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};
use std::time::Duration;

use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tauri::{AppHandle, Emitter, Manager};

use crate::commands::{err, CmdResult};

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Provider {
    pub id: String,
    /// Which preset it came from ("openai", "deepseek", "ollama"…).
    pub preset: String,
    /// What the person calls it.
    pub name: String,
    /// "openai" (chat completions), "anthropic" (messages) or "hyperlume"
    /// (the `hyperlume-ai` program on a Hyperlume desktop).
    pub protocol: String,
    pub base_url: String,
    pub model: String,
    /// Hyperlume AI only: which of Hyperlume's providers to use ("local",
    /// "openai"…). Empty follows Hyperlume's own Settings → AI.
    pub upstream: String,
    /// Filled in when sent to the window; never stored.
    pub has_key: bool,
}

#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct AiConfig {
    pub providers: Vec<Provider>,
    pub default_id: Option<String>,
    /// The model used to search by meaning, if one is set up.
    pub embed: Option<EmbedSetting>,
    /// Hyperlume AI has been added once, so it isn't added again by itself
    /// after the person removes it.
    pub hyperlume_offered: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct EmbedSetting {
    pub provider_id: String,
    pub model: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Msg {
    pub role: String,
    pub content: String,
}

// ------------------------------------------------------------ settings

fn config_dir(app: &AppHandle) -> CmdResult<PathBuf> {
    match crate::portable::data("config") {
        Some(d) => Ok(d),
        None => app.path().app_config_dir().map_err(err),
    }
}

pub(crate) fn load(app: &AppHandle) -> AiConfig {
    config_dir(app)
        .ok()
        .and_then(|d| std::fs::read(d.join("ai.json")).ok())
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_default()
}

pub(crate) fn save(app: &AppHandle, cfg: &AiConfig) -> CmdResult<()> {
    let dir = config_dir(app)?;
    std::fs::create_dir_all(&dir).map_err(err)?;
    let mut clean = AiConfig {
        providers: cfg.providers.clone(),
        default_id: cfg.default_id.clone(),
        embed: cfg.embed.clone(),
        hyperlume_offered: cfg.hyperlume_offered,
    };
    for p in &mut clean.providers {
        p.has_key = false;
    }
    marrow_core::atomic::write(dir.join("ai.json"), serde_json::to_vec_pretty(&clean).map_err(err)?).map_err(err)
}

// ----------------------------------------------------------------- keys

/// Secrets (API keys, sync passwords) in the system's credential store.
pub(crate) mod keys {
    use super::*;

    #[cfg(any(windows, target_os = "macos"))]
    const SERVICE: &str = "Marrow AI";

    fn file(app: &AppHandle) -> Option<PathBuf> {
        config_dir(app).ok().map(|d| d.join("ai-keys.json"))
    }

    fn read_file(app: &AppHandle) -> HashMap<String, String> {
        file(app)
            .and_then(|p| std::fs::read(p).ok())
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default()
    }

    fn write_file(app: &AppHandle, map: &HashMap<String, String>) -> CmdResult<()> {
        let p = file(app).ok_or("No settings folder")?;
        if let Some(d) = p.parent() {
            std::fs::create_dir_all(d).map_err(err)?;
        }
        if map.is_empty() {
            let _ = std::fs::remove_file(&p);
            return Ok(());
        }
        marrow_core::atomic::write(p, serde_json::to_vec_pretty(map).map_err(err)?).map_err(err)
    }

    /// Whether keys go in the system's credential store.
    fn use_os() -> bool {
        cfg!(any(windows, target_os = "macos")) && crate::portable::root().is_none()
    }

    #[cfg(any(windows, target_os = "macos"))]
    fn os_set(id: &str, key: Option<&str>) -> Result<(), String> {
        let e = keyring::Entry::new(SERVICE, id).map_err(err)?;
        match key {
            Some(k) => e.set_password(k).map_err(err),
            None => match e.delete_credential() {
                Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
                Err(x) => Err(err(x)),
            },
        }
    }
    #[cfg(not(any(windows, target_os = "macos")))]
    fn os_set(_id: &str, _key: Option<&str>) -> Result<(), String> {
        Err("no credential store".into())
    }

    #[cfg(any(windows, target_os = "macos"))]
    fn os_get(id: &str) -> Option<String> {
        keyring::Entry::new(SERVICE, id).ok()?.get_password().ok()
    }
    #[cfg(not(any(windows, target_os = "macos")))]
    fn os_get(_id: &str) -> Option<String> {
        None
    }

    pub fn set(app: &AppHandle, id: &str, key: Option<&str>) -> CmdResult<()> {
        let mut map = read_file(app);
        if use_os() && os_set(id, key).is_ok() {
            // Not in the file as well.
            if map.remove(id).is_some() {
                write_file(app, &map)?;
            }
            return Ok(());
        }
        match key {
            Some(k) => map.insert(id.to_string(), k.to_string()),
            None => map.remove(id),
        };
        write_file(app, &map)
    }

    pub fn get(app: &AppHandle, id: &str) -> Option<String> {
        if use_os() {
            if let Some(k) = os_get(id) {
                return Some(k);
            }
        }
        read_file(app).get(id).cloned()
    }
}

// --------------------------------------------------------- transcription

/// Services that turn speech into text, and the model each one uses.
const TRANSCRIBERS: &[(&str, &str)] = &[("openai", "gpt-4o-mini-transcribe"), ("groq", "whisper-large-v3-turbo")];

/// The AI that can transcribe a voice note, if one is set up (by name), so the
/// window can say where the recording would go.
#[tauri::command(async)]
pub fn ai_transcriber(app: AppHandle) -> Option<String> {
    transcriber(&app).map(|(p, _, _)| p.name)
}

fn transcriber(app: &AppHandle) -> Option<(Provider, &'static str, String)> {
    let cfg = load(app);
    TRANSCRIBERS.iter().find_map(|(preset, model)| {
        let p = cfg.providers.iter().find(|p| p.preset == *preset)?.clone();
        let key = keys::get(app, &p.id)?;
        Some((p, *model, key))
    })
}

/// Turns a voice note into text with the person's own AI service (OpenAI or
/// Groq). The recording's bytes come as the request body; `name` names it.
/// None when no such service is set up.
#[tauri::command]
pub async fn ai_transcribe(app: AppHandle, request: tauri::ipc::Request<'_>) -> CmdResult<Option<String>> {
    let tauri::ipc::InvokeBody::Raw(bytes) = request.body() else {
        return Err("Expected the recording's bytes".into());
    };
    let name = request
        .headers()
        .get("name")
        .and_then(|v| v.to_str().ok())
        .map(crate::commands::percent_decode)
        .unwrap_or_else(|| "voice.webm".into());
    let Some((p, model, key)) = transcriber(&app) else { return Ok(None) };
    let part = reqwest::multipart::Part::bytes(bytes.clone()).file_name(name);
    let form = reqwest::multipart::Form::new().text("model", model.to_string()).part("file", part);
    let resp = client()?
        .post(url(&p.base_url, "audio/transcriptions"))
        .bearer_auth(key)
        .multipart(form)
        .timeout(Duration::from_secs(120))
        .send()
        .await
        .map_err(|e| friendly(e, &p))?;
    if !resp.status().is_success() {
        return Err(failure(resp).await);
    }
    let v: Value = resp.json().await.map_err(err)?;
    Ok(v.get("text").and_then(|t| t.as_str()).map(|t| t.trim().to_string()))
}

// -------------------------------------------------------------- requests

pub(crate) fn client() -> CmdResult<reqwest::Client> {
    reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(20))
        .user_agent(concat!("Marrow/", env!("CARGO_PKG_VERSION")))
        .build()
        .map_err(err)
}

pub(crate) fn url(base: &str, path: &str) -> String {
    format!("{}/{}", base.trim().trim_end_matches('/'), path)
}

fn build(
    p: &Provider,
    key: Option<&str>,
    system: &str,
    messages: &[Msg],
    stream: bool,
) -> CmdResult<reqwest::RequestBuilder> {
    let c = client()?;
    if p.protocol == "anthropic" {
        let mut body = json!({
            "model": p.model,
            "max_tokens": 4096,
            "messages": messages,
            "stream": stream,
        });
        if !system.is_empty() {
            body["system"] = json!(system);
        }
        let mut r = c
            .post(url(&p.base_url, "messages"))
            .header("anthropic-version", "2023-06-01")
            .json(&body);
        if let Some(k) = key {
            r = r.header("x-api-key", k);
        }
        Ok(r)
    } else {
        let mut all = Vec::with_capacity(messages.len() + 1);
        if !system.is_empty() {
            all.push(json!({ "role": "system", "content": system }));
        }
        all.extend(messages.iter().map(|m| json!({ "role": m.role, "content": m.content })));
        let mut r = c
            .post(url(&p.base_url, "chat/completions"))
            .json(&json!({ "model": p.model, "messages": all, "stream": stream }));
        if let Some(k) = key {
            r = r.bearer_auth(k);
        }
        Ok(r)
    }
}

/// The provider's own words for what went wrong.
pub(crate) async fn failure(resp: reqwest::Response) -> String {
    let status = resp.status();
    let body = resp.text().await.unwrap_or_default();
    let msg = serde_json::from_str::<Value>(&body)
        .ok()
        .and_then(|v| {
            v.pointer("/error/message")
                .or_else(|| v.pointer("/error"))
                .or_else(|| v.pointer("/message"))
                .or_else(|| v.pointer("/msg"))
                .and_then(|m| m.as_str().map(str::to_string))
        })
        .unwrap_or_else(|| body.chars().take(300).collect());
    let hint = match status.as_u16() {
        401 | 403 => " (check the API key)",
        404 => " (check the address and the model name)",
        429 => " (rate limit or no credit left)",
        _ => "",
    };
    format!("{status}{hint}: {msg}")
}

pub(crate) fn friendly(e: reqwest::Error, p: &Provider) -> String {
    if e.is_connect() {
        if p.preset == "ollama" || p.preset == "lmstudio" || p.base_url.contains("localhost") || p.base_url.contains("127.0.0.1") {
            return format!("Couldn't reach {} at {}. Is it running?", p.name, p.base_url);
        }
        return format!("Couldn't reach {}. Check the internet connection and the address.", p.name);
    }
    if e.is_timeout() {
        return format!("{} took too long to answer.", p.name);
    }
    e.to_string()
}

/// The text a streamed event adds, for either protocol.
fn delta(protocol: &str, v: &Value) -> Option<String> {
    if protocol == "anthropic" {
        if v.get("type")?.as_str()? == "content_block_delta" {
            return v.pointer("/delta/text")?.as_str().map(str::to_string);
        }
        return None;
    }
    v.pointer("/choices/0/delta/content")?.as_str().map(str::to_string)
}

/// An error sent inside a stream.
fn stream_error(v: &Value) -> Option<String> {
    let e = v.get("error")?;
    Some(e.get("message").and_then(|m| m.as_str()).map(str::to_string).unwrap_or_else(|| e.to_string()))
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct AiEvent {
    id: String,
    /// "chunk", "done" or "error".
    kind: &'static str,
    text: String,
}

async fn stream(app: &AppHandle, id: &str, p: &Provider, key: Option<String>, system: &str, messages: &[Msg]) -> Result<(), String> {
    let resp = build(p, key.as_deref(), system, messages, true)?
        .send()
        .await
        .map_err(|e| friendly(e, p))?;
    if !resp.status().is_success() {
        return Err(failure(resp).await);
    }
    let mut body = resp.bytes_stream();
    let mut buf = String::new();
    while let Some(chunk) = body.next().await {
        let chunk = chunk.map_err(|e| friendly(e, p))?;
        buf.push_str(&String::from_utf8_lossy(&chunk));
        while let Some(nl) = buf.find('\n') {
            let line = buf[..nl].trim().to_string();
            buf.drain(..=nl);
            let Some(data) = line.strip_prefix("data:") else { continue };
            let data = data.trim();
            if data == "[DONE]" {
                return Ok(());
            }
            let Ok(v) = serde_json::from_str::<Value>(data) else { continue };
            if let Some(e) = stream_error(&v) {
                return Err(e);
            }
            if let Some(t) = delta(&p.protocol, &v) {
                if !t.is_empty() {
                    let _ = app.emit("ai-event", AiEvent { id: id.to_string(), kind: "chunk", text: t });
                }
            }
        }
    }
    Ok(())
}

fn running() -> &'static Mutex<HashMap<String, tauri::async_runtime::JoinHandle<()>>> {
    static R: OnceLock<Mutex<HashMap<String, tauri::async_runtime::JoinHandle<()>>>> = OnceLock::new();
    R.get_or_init(Default::default)
}

fn pick(app: &AppHandle, provider_id: Option<&str>) -> CmdResult<Provider> {
    let cfg = load(app);
    let want = provider_id.map(str::to_string).or(cfg.default_id.clone());
    cfg.providers
        .iter()
        .find(|p| Some(&p.id) == want.as_ref())
        .or(cfg.providers.first())
        .cloned()
        .ok_or_else(|| "No AI is set up yet. Add one in Settings → AI.".to_string())
}

// --------------------------------------------------------------- commands

#[tauri::command(async)]
pub fn ai_config(app: AppHandle) -> AiConfig {
    let mut cfg = load(&app);
    for p in &mut cfg.providers {
        p.has_key = keys::get(&app, &p.id).is_some();
    }
    cfg
}

/// Adds or updates a provider. `key`: a new key, "" to remove it, or absent to keep it.
#[tauri::command(async)]
pub fn ai_save_provider(app: AppHandle, mut provider: Provider, key: Option<String>) -> CmdResult<AiConfig> {
    let mut cfg = load(&app);
    if provider.id.is_empty() {
        let n = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis())
            .unwrap_or(0);
        provider.id = format!("{}-{n}", if provider.preset.is_empty() { "custom" } else { &provider.preset });
    }
    provider.base_url = provider.base_url.trim().trim_end_matches('/').to_string();
    provider.model = provider.model.trim().to_string();
    provider.upstream = provider.upstream.trim().to_string();
    if provider.protocol == HYPERLUME {
        cfg.hyperlume_offered = true;
    }
    match key.as_deref().map(str::trim) {
        Some("") => keys::set(&app, &provider.id, None)?,
        Some(k) => keys::set(&app, &provider.id, Some(k))?,
        None => {}
    }
    match cfg.providers.iter_mut().find(|p| p.id == provider.id) {
        Some(p) => *p = provider.clone(),
        None => cfg.providers.push(provider.clone()),
    }
    if cfg.default_id.as_ref().is_none_or(|d| !cfg.providers.iter().any(|p| &p.id == d)) {
        cfg.default_id = Some(provider.id.clone());
    }
    save(&app, &cfg)?;
    Ok(ai_config(app))
}

#[tauri::command(async)]
pub fn ai_remove_provider(app: AppHandle, id: String) -> CmdResult<AiConfig> {
    let mut cfg = load(&app);
    cfg.providers.retain(|p| p.id != id);
    keys::set(&app, &id, None)?;
    if cfg.default_id.as_deref() == Some(&id) {
        cfg.default_id = cfg.providers.first().map(|p| p.id.clone());
    }
    if cfg.embed.as_ref().is_some_and(|e| e.provider_id == id) {
        cfg.embed = None;
    }
    save(&app, &cfg)?;
    Ok(ai_config(app))
}

#[tauri::command(async)]
pub fn ai_set_default(app: AppHandle, id: String) -> CmdResult<AiConfig> {
    let mut cfg = load(&app);
    if cfg.providers.iter().any(|p| p.id == id) {
        cfg.default_id = Some(id);
        save(&app, &cfg)?;
    }
    Ok(ai_config(app))
}

/// The key to use while editing: the one typed, else the saved one.
pub(crate) fn key_for(app: &AppHandle, p: &Provider, key: Option<String>) -> Option<String> {
    key.filter(|k| !k.trim().is_empty())
        .map(|k| k.trim().to_string())
        .or_else(|| (!p.id.is_empty()).then(|| keys::get(app, &p.id)).flatten())
}

/// The models a provider offers (for the picker).
#[tauri::command]
pub async fn ai_models(app: AppHandle, provider: Provider, key: Option<String>) -> CmdResult<Vec<String>> {
    if provider.protocol == HYPERLUME {
        // Hyperlume picks its own model unless the person types one.
        return Ok(Vec::new());
    }
    let key = key_for(&app, &provider, key);
    let c = client()?;
    let mut req = c.get(url(&provider.base_url, "models")).timeout(Duration::from_secs(20));
    if provider.protocol == "anthropic" {
        req = req.header("anthropic-version", "2023-06-01");
        if let Some(k) = &key {
            req = req.header("x-api-key", k);
        }
    } else if let Some(k) = &key {
        req = req.bearer_auth(k);
    }
    let resp = req.send().await.map_err(|e| friendly(e, &provider))?;
    if !resp.status().is_success() {
        return Err(failure(resp).await);
    }
    let v: Value = resp.json().await.map_err(err)?;
    let list = v
        .get("data")
        .or_else(|| v.get("models"))
        .and_then(|d| d.as_array())
        .cloned()
        .unwrap_or_default();
    let mut ids: Vec<String> = list
        .iter()
        .filter_map(|m| m.get("id").or_else(|| m.get("name")).and_then(|x| x.as_str()))
        .map(|s| s.trim_start_matches("models/").to_string())
        .collect();
    ids.sort();
    ids.dedup();
    Ok(ids)
}

/// Sends a tiny request and returns the reply, to check the settings work.
#[tauri::command]
pub async fn ai_test(app: AppHandle, provider: Provider, key: Option<String>) -> CmdResult<String> {
    let msgs = [Msg {
        role: "user".into(),
        content: "Reply with just the word: ready".into(),
    }];
    if provider.protocol == HYPERLUME {
        return hyperlume_test(&provider, &msgs).await;
    }
    let key = key_for(&app, &provider, key);
    if provider.model.trim().is_empty() {
        return Err("Pick a model first.".into());
    }
    let resp = build(&provider, key.as_deref(), "", &msgs, false)?
        .timeout(Duration::from_secs(90))
        .send()
        .await
        .map_err(|e| friendly(e, &provider))?;
    if !resp.status().is_success() {
        return Err(failure(resp).await);
    }
    let v: Value = resp.json().await.map_err(err)?;
    let text = if provider.protocol == "anthropic" {
        v.pointer("/content/0/text").and_then(|t| t.as_str())
    } else {
        v.pointer("/choices/0/message/content").and_then(|t| t.as_str())
    }
    .unwrap_or("")
    .trim()
    .chars()
    .take(200)
    .collect::<String>();
    Ok(if text.is_empty() { "Connected.".into() } else { text })
}

/// Starts a request; the answer arrives as `ai-event`s with this `id`.
#[tauri::command(async)]
pub fn ai_run(app: AppHandle, id: String, provider_id: Option<String>, system: String, messages: Vec<Msg>) -> CmdResult<String> {
    let p = pick(&app, provider_id.as_deref())?;
    if p.protocol == HYPERLUME {
        return hyperlume_run(app, id, p, system, messages);
    }
    if p.model.is_empty() {
        return Err(format!("{} has no model chosen. Pick one in Settings → AI.", p.name));
    }
    let key = keys::get(&app, &p.id);
    let name = p.name.clone();
    let app2 = app.clone();
    let id2 = id.clone();
    let task = tauri::async_runtime::spawn(async move {
        let result = stream(&app2, &id2, &p, key, &system, &messages).await;
        let ev = match result {
            Ok(()) => AiEvent { id: id2.clone(), kind: "done", text: String::new() },
            Err(e) => AiEvent { id: id2.clone(), kind: "error", text: e },
        };
        let _ = app2.emit("ai-event", ev);
        if let Ok(mut r) = running().lock() {
            r.remove(&id2);
        }
    });
    if let Ok(mut r) = running().lock() {
        r.insert(id, task);
    }
    Ok(name)
}

#[tauri::command(async)]
pub fn ai_cancel(id: String) {
    // Hyperlume AI is stopped with SIGTERM; its task then ends by itself.
    #[cfg(target_os = "linux")]
    if let Some(c) = hyper_running().lock().ok().and_then(|mut r| r.remove(&id)) {
        c.cancel();
        return;
    }
    if let Ok(mut r) = running().lock() {
        if let Some(t) = r.remove(&id) {
            t.abort();
        }
    }
}

// ------------------------------------------------------------ Hyperlume AI

/// The protocol of the Hyperlume AI provider.
pub(crate) const HYPERLUME: &str = "hyperlume";

/// Whether Hyperlume AI can be used here: only on Linux, and only when this
/// computer's `hyperlume-ai` offers plain chat. Asked once, then remembered.
#[tauri::command]
pub async fn ai_hyperlume_available() -> bool {
    hyperlume_available().await
}

#[cfg(target_os = "linux")]
async fn hyperlume_available() -> bool {
    hyperlume::available().await
}

#[cfg(not(target_os = "linux"))]
async fn hyperlume_available() -> bool {
    false
}

#[cfg(target_os = "linux")]
fn hyper_running() -> &'static Mutex<HashMap<String, std::sync::Arc<hyperlume::Cancel>>> {
    static R: OnceLock<Mutex<HashMap<String, std::sync::Arc<hyperlume::Cancel>>>> = OnceLock::new();
    R.get_or_init(Default::default)
}

#[cfg(target_os = "linux")]
fn hyperlume_run(app: AppHandle, id: String, p: Provider, system: String, messages: Vec<Msg>) -> CmdResult<String> {
    let cancel = std::sync::Arc::new(hyperlume::Cancel::default());
    if let Ok(mut r) = hyper_running().lock() {
        r.insert(id.clone(), cancel.clone());
    }
    let name = p.name.clone();
    tauri::async_runtime::spawn(async move {
        let path = std::env::var_os("PATH").unwrap_or_default();
        hyperlume::run(&path, &p.upstream, &p.model, &system, &messages, &cancel, |kind, text| {
            let _ = app.emit("ai-event", AiEvent { id: id.clone(), kind, text });
        })
        .await;
        if let Ok(mut r) = hyper_running().lock() {
            r.remove(&id);
        }
    });
    Ok(name)
}

#[cfg(not(target_os = "linux"))]
fn hyperlume_run(_app: AppHandle, _id: String, _p: Provider, _system: String, _messages: Vec<Msg>) -> CmdResult<String> {
    Err(NOT_HERE.into())
}

#[cfg(not(target_os = "linux"))]
const NOT_HERE: &str = "Hyperlume AI works only on a Hyperlume desktop. Pick another AI in Settings → AI.";

#[cfg(target_os = "linux")]
async fn hyperlume_test(p: &Provider, msgs: &[Msg]) -> CmdResult<String> {
    let path = std::env::var_os("PATH").unwrap_or_default();
    let text = hyperlume::ask_with_limit(&path, &p.upstream, &p.model, msgs, Duration::from_secs(90)).await?;
    let text: String = text.trim().chars().take(200).collect();
    Ok(if text.is_empty() { "Connected.".into() } else { text })
}

#[cfg(not(target_os = "linux"))]
async fn hyperlume_test(_p: &Provider, _msgs: &[Msg]) -> CmdResult<String> {
    Err(NOT_HERE.into())
}

/// Hyperlume AI: on a Hyperlume desktop, the same AI as its terminal helper,
/// local or cloud, as chosen in Hyperlume's Settings → AI. Marrow runs
/// `hyperlume-ai chat --plain`, writes the conversation to it as JSON and
/// reads the answer back as JSON lines. No key or address is needed here.
#[cfg(target_os = "linux")]
pub(crate) mod hyperlume {
    use super::Msg;
    use serde_json::{json, Value};
    use std::ffi::OsStr;
    use std::os::unix::fs::PermissionsExt;
    use std::path::PathBuf;
    use std::process::Stdio;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::OnceLock;
    use std::time::Duration;
    use tokio::io::{AsyncBufRead, AsyncBufReadExt, AsyncWriteExt, BufReader};
    use tokio::process::{Child, Command};
    use tokio::sync::Notify;

    pub const BIN: &str = "hyperlume-ai";
    /// `capabilities` answers within 1 s; this leaves room for a busy computer.
    pub const PROBE_TIMEOUT: Duration = Duration::from_secs(3);
    /// After SIGTERM, Hyperlume exits within 2 s; then it's killed.
    const GRACE: Duration = Duration::from_secs(3);

    const NOT_INSTALLED: &str = "Hyperlume AI isn't installed on this computer.";

    /// The `hyperlume-ai` program on this PATH, if there is one.
    pub fn find(path: &OsStr) -> Option<PathBuf> {
        std::env::split_paths(path)
            .map(|d| d.join(BIN))
            .find(|f| f.metadata().is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0))
    }

    /// Whether this `hyperlume-ai` can answer plain chat requests
    /// (`capabilities` exits 0 and says `"api": 1, "chat_plain": true`).
    pub async fn probe(path: &OsStr, timeout: Duration) -> bool {
        let Some(bin) = find(path) else { return false };
        let child = Command::new(bin)
            .arg("capabilities")
            .env("PATH", path)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .kill_on_drop(true)
            .spawn();
        let Ok(child) = child else { return false };
        let Ok(Ok(out)) = tokio::time::timeout(timeout, child.wait_with_output()).await else { return false };
        out.status.success()
            && serde_json::from_slice::<Value>(&out.stdout)
                .ok()
                .and_then(|v| Some(v.get("api")?.as_u64()? == 1 && v.get("chat_plain")?.as_bool()?))
                .unwrap_or(false)
    }

    /// Asked once per run of Marrow, then remembered.
    pub async fn available() -> bool {
        static CACHE: OnceLock<bool> = OnceLock::new();
        if let Some(v) = CACHE.get() {
            return *v;
        }
        let v = probe(&std::env::var_os("PATH").unwrap_or_default(), PROBE_TIMEOUT).await;
        *CACHE.get_or_init(|| v)
    }

    /// The command line: the provider and model only when the person picked them.
    pub fn args(upstream: &str, model: &str) -> Vec<String> {
        let mut a = vec!["chat".to_string(), "--plain".to_string()];
        for (flag, v) in [("--provider", upstream.trim()), ("--model", model.trim())] {
            if !v.is_empty() {
                a.push(flag.into());
                a.push(v.into());
            }
        }
        a
    }

    /// Stops a running answer. `chat` itself sends SIGTERM, through the
    /// child it still owns, so a reaped (and maybe reused) pid is never signalled.
    #[derive(Default)]
    pub struct Cancel {
        cancelled: AtomicBool,
        notify: Notify,
    }

    impl Cancel {
        pub fn cancel(&self) {
            self.cancelled.store(true, Ordering::SeqCst);
            self.notify.notify_one();
        }

        pub fn is_cancelled(&self) -> bool {
            self.cancelled.load(Ordering::SeqCst)
        }
    }

    /// What an exit code means, for when Hyperlume gave no sentence of its own.
    pub fn exit_hint(code: Option<i32>) -> String {
        match code {
            Some(1) => "Hyperlume AI couldn't read the request, or doesn't know the chosen provider.",
            Some(2) => "Hyperlume AI is off, or no local model is chosen. Turn it on in Hyperlume's Settings → AI.",
            Some(3) => "There's no key for the cloud AI chosen in Hyperlume. Add it in Hyperlume's Settings → AI.",
            Some(4) => "Hyperlume AI's engine or provider failed. Try again in a moment.",
            _ => "Hyperlume AI stopped without answering.",
        }
        .to_string()
    }

    /// The longest line read; anything longer is an error (1 MiB of text per chunk is plenty).
    pub const MAX_LINE: usize = 1 << 20;
    const UNREADABLE: &str = "Marrow couldn't read Hyperlume AI's answer.";

    /// How reading the answer ended.
    #[derive(Debug)]
    pub enum Read {
        /// The output ended.
        End { done: bool, error: Option<String> },
        Cancelled,
        /// The output couldn't be read (a read error, text that isn't UTF-8, a line too long).
        Broken(String),
    }

    enum Line {
        Eof,
        Full,
        TooLong,
    }

    /// Reads one line into `buf` (without the newline), never holding more than MAX_LINE.
    async fn next_line<R: AsyncBufRead + Unpin>(r: &mut R, buf: &mut Vec<u8>) -> std::io::Result<Line> {
        buf.clear();
        loop {
            let avail = r.fill_buf().await?;
            if avail.is_empty() {
                return Ok(if buf.is_empty() { Line::Eof } else { Line::Full });
            }
            let (take, end) = match avail.iter().position(|&b| b == b'\n') {
                Some(i) => (i, true),
                None => (avail.len(), false),
            };
            if buf.len() + take > MAX_LINE {
                return Ok(Line::TooLong);
            }
            buf.extend_from_slice(&avail[..take]);
            r.consume(take + usize::from(end));
            if end {
                return Ok(Line::Full);
            }
        }
    }

    /// Reads the JSON lines of an answer; `on_chunk` gets each piece of text.
    pub async fn read_answer<R: AsyncBufRead + Unpin>(mut r: R, cancel: &Cancel, mut on_chunk: impl FnMut(String)) -> Read {
        let (mut done, mut error) = (false, None::<String>);
        let mut buf = Vec::new();
        loop {
            let got = tokio::select! {
                l = next_line(&mut r, &mut buf) => l,
                _ = cancel.notify.notified() => return Read::Cancelled,
            };
            if cancel.is_cancelled() {
                return Read::Cancelled;
            }
            match got {
                Ok(Line::Eof) => return Read::End { done, error },
                Ok(Line::TooLong) => return Read::Broken(format!("{UNREADABLE} A line was too long.")),
                Err(_) => return Read::Broken(UNREADABLE.into()),
                Ok(Line::Full) => {}
            }
            let Ok(line) = std::str::from_utf8(&buf) else {
                return Read::Broken(format!("{UNREADABLE} It wasn't UTF-8 text."));
            };
            let Ok(v) = serde_json::from_str::<Value>(line.trim()) else { continue };
            if let Some(t) = v.get("d").and_then(|d| d.as_str()) {
                if !t.is_empty() {
                    on_chunk(t.to_string());
                }
            } else if let Some(e) = v.get("error") {
                let e = e.as_str().map(str::trim).filter(|e| !e.is_empty());
                error = Some(e.map(str::to_string).unwrap_or_else(|| exit_hint(None)));
            } else if v.get("done").and_then(|d| d.as_bool()) == Some(true) {
                done = true;
            }
        }
    }

    /// SIGTERM through the child we still own (`id()` is None once it's
    /// reaped), a grace period, then SIGKILL; always reaps it.
    async fn stop(child: &mut Child) -> std::io::Result<std::process::ExitStatus> {
        if let Some(pid) = child.id().and_then(|p| i32::try_from(p).ok()) {
            // SAFETY: kill() only sends a signal, to our own child that hasn't been reaped.
            unsafe {
                libc::kill(pid, libc::SIGTERM);
            }
        }
        match tokio::time::timeout(GRACE, child.wait()).await {
            Ok(s) => s,
            Err(_) => {
                let _ = child.start_kill();
                child.wait().await
            }
        }
    }

    /// Asks Hyperlume AI; `on_chunk` gets the answer as it comes. After
    /// `cancel`, no more text is passed on and the result doesn't matter.
    pub async fn chat(
        path: &OsStr,
        upstream: &str,
        model: &str,
        system: &str,
        messages: &[Msg],
        cancel: &Cancel,
        on_chunk: impl FnMut(String),
    ) -> Result<(), String> {
        let bin = find(path).ok_or(NOT_INSTALLED)?;
        let mut child = Command::new(bin)
            .args(args(upstream, model))
            .env("PATH", path)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .kill_on_drop(true)
            .spawn()
            .map_err(|_| NOT_INSTALLED)?;

        let mut all = Vec::with_capacity(messages.len() + 1);
        if !system.is_empty() {
            all.push(json!({ "role": "system", "content": system }));
        }
        all.extend(messages.iter().map(|m| json!({ "role": m.role, "content": m.content })));
        let body = serde_json::to_vec(&all).map_err(|e| e.to_string())?;
        // Written on the side, so a program that doesn't read can't hold up
        // the answer; stdin is closed when it's written (or the program exits).
        if let Some(mut stdin) = child.stdin.take() {
            tauri::async_runtime::spawn(async move {
                let _ = stdin.write_all(&body).await;
                let _ = stdin.shutdown().await;
            });
        }

        let read = match child.stdout.take() {
            Some(out) if !cancel.is_cancelled() => read_answer(BufReader::new(out), cancel, on_chunk).await,
            Some(_) => Read::Cancelled,
            None => Read::Broken(UNREADABLE.into()),
        };
        // stdout is closed by now, so a program still writing gets EPIPE
        // instead of blocking. Then it's reaped, in every case.
        let status = match &read {
            Read::End { .. } => tokio::select! {
                s = child.wait() => s,
                _ = cancel.notify.notified() => stop(&mut child).await,
            },
            Read::Cancelled | Read::Broken(_) => stop(&mut child).await,
        };
        if cancel.is_cancelled() {
            return Ok(());
        }
        let (done, error) = match read {
            Read::End { done, error } => (done, error),
            Read::Broken(e) => return Err(e),
            Read::Cancelled => return Ok(()),
        };
        let status = status.map_err(|e| e.to_string())?;
        if let Some(e) = error {
            return Err(e);
        }
        if !status.success() {
            return Err(exit_hint(status.code()));
        }
        if !done {
            return Err("Hyperlume AI stopped before the answer was finished.".into());
        }
        Ok(())
    }

    /// Runs a chat and reports it as events: ("chunk", text)…, then exactly
    /// one ("done", "") or ("error", sentence), or nothing more after a cancel.
    pub async fn run(
        path: &OsStr,
        upstream: &str,
        model: &str,
        system: &str,
        messages: &[Msg],
        cancel: &Cancel,
        mut emit: impl FnMut(&'static str, String),
    ) {
        let result = chat(path, upstream, model, system, messages, cancel, |t| emit("chunk", t)).await;
        if cancel.is_cancelled() {
            return;
        }
        match result {
            Ok(()) => emit("done", String::new()),
            Err(e) => emit("error", e),
        }
    }

    /// The whole answer, or an error after `limit`. A late answer is stopped
    /// the contract's way (SIGTERM, then a kill after the grace period), not dropped.
    pub async fn ask_with_limit(path: &OsStr, upstream: &str, model: &str, messages: &[Msg], limit: Duration) -> Result<String, String> {
        let cancel = Cancel::default();
        let mut text = String::new();
        let in_time = {
            let asked = chat(path, upstream, model, "", messages, &cancel, |t| text.push_str(&t));
            tokio::pin!(asked);
            tokio::select! {
                r = &mut asked => Some(r),
                _ = tokio::time::sleep(limit) => {
                    cancel.cancel();
                    let _ = asked.await;
                    None
                }
            }
        };
        match in_time {
            Some(r) => r.map(|()| text),
            None => Err("Hyperlume AI took too long to answer.".into()),
        }
    }
}

/// Hyperlume AI, against a fake `hyperlume-ai` script put on a PATH of its own.
#[cfg(all(test, target_os = "linux"))]
mod hyperlume_tests {
    use super::hyperlume::{self, Cancel};
    use super::Msg;
    use std::ffi::OsString;
    use std::os::unix::fs::PermissionsExt;
    use std::path::Path;
    use std::sync::Arc;
    use std::time::{Duration, Instant};

    /// A PATH with the fake `hyperlume-ai` and the few tools its script needs.
    fn fake(dir: &Path, script: &str) -> OsString {
        let bin = dir.join("hyperlume-ai");
        std::fs::write(&bin, format!("#!/bin/sh\nD='{}'\n{script}\n", dir.display())).unwrap();
        std::fs::set_permissions(&bin, std::fs::Permissions::from_mode(0o755)).unwrap();
        path_with(dir)
    }

    /// `dir`, then a folder with only a few basic tools: never the system's
    /// folders, which may hold a real `hyperlume-ai`.
    fn path_with(dir: &Path) -> OsString {
        let tools = dir.join("tools");
        std::fs::create_dir_all(&tools).unwrap();
        for t in ["cat", "sleep", "head", "tr"] {
            let to = tools.join(t);
            if !to.exists() {
                let from = ["/usr/bin", "/bin"].iter().map(|d| Path::new(d).join(t)).find(|p| p.exists()).unwrap();
                std::os::unix::fs::symlink(from, to).unwrap();
            }
        }
        format!("{}:{}", dir.display(), tools.display()).into()
    }

    fn block<T>(f: impl std::future::Future<Output = T>) -> T {
        tauri::async_runtime::block_on(f)
    }

    /// One test at a time: writing a script while another thread forks can
    /// make exec fail with "Text file busy" (rust-lang/rust#114554).
    fn serial() -> std::sync::MutexGuard<'static, ()> {
        static M: std::sync::Mutex<()> = std::sync::Mutex::new(());
        M.lock().unwrap_or_else(|p| p.into_inner())
    }

    /// Runs to the end through the event path, with a time limit; returns
    /// the events as (kind, text).
    fn events(path: &OsString) -> Vec<(&'static str, String)> {
        let mut ev = Vec::new();
        let cancel = Cancel::default();
        let msgs = ask("Hi");
        let r = block(async {
            tokio::time::timeout(Duration::from_secs(10), hyperlume::run(path, "", "", "", &msgs, &cancel, |k, t| ev.push((k, t)))).await
        });
        assert!(r.is_ok(), "no hang");
        ev
    }

    fn errors(ev: &[(&'static str, String)]) -> Vec<String> {
        ev.iter().filter(|(k, _)| *k == "error").map(|(_, t)| t.clone()).collect()
    }

    fn ask(text: &str) -> Vec<Msg> {
        vec![Msg { role: "user".into(), content: text.into() }]
    }

    /// Runs a chat to the end; returns the chunks and the result.
    fn run(path: &OsString, upstream: &str, model: &str) -> (Vec<String>, Result<(), String>) {
        let mut chunks = Vec::new();
        let cancel = Cancel::default();
        let r = block(hyperlume::chat(path, upstream, model, "Be brief.", &ask("Hi"), &cancel, |t| chunks.push(t)));
        (chunks, r)
    }

    const CAPS: &str = r#"if [ "$1" = capabilities ]; then echo '{"api": 1, "chat_plain": true, "providers_cmd": true}'; exit 0; fi"#;

    #[test]
    fn capabilities_present() {
        let _g = serial();
        let d = tempfile::tempdir().unwrap();
        let path = fake(d.path(), CAPS);
        assert!(hyperlume::find(&path).is_some());
        assert!(block(hyperlume::probe(&path, hyperlume::PROBE_TIMEOUT)));
    }

    #[test]
    fn capabilities_missing() {
        let _g = serial();
        // No program at all (an older Hyperlume, or not Hyperlume).
        let d = tempfile::tempdir().unwrap();
        let path = path_with(d.path());
        assert!(hyperlume::find(&path).is_none());
        assert!(!block(hyperlume::probe(&path, hyperlume::PROBE_TIMEOUT)));
        // A program that doesn't know the command.
        let path = fake(d.path(), "echo 'usage: hyperlume-ai …' >&2; exit 1");
        assert!(!block(hyperlume::probe(&path, hyperlume::PROBE_TIMEOUT)));
        // A program that knows it but can't do plain chat.
        let path = fake(d.path(), r#"echo '{"api": 1, "chat_plain": false}'"#);
        assert!(!block(hyperlume::probe(&path, hyperlume::PROBE_TIMEOUT)));
        // Another version of the contract, or none stated.
        let path = fake(d.path(), r#"echo '{"api": 2, "chat_plain": true}'"#);
        assert!(!block(hyperlume::probe(&path, hyperlume::PROBE_TIMEOUT)));
        let path = fake(d.path(), r#"echo '{"chat_plain": true}'"#);
        assert!(!block(hyperlume::probe(&path, hyperlume::PROBE_TIMEOUT)));
        // The right answer, but a failing exit.
        let path = fake(d.path(), r#"echo '{"api": 1, "chat_plain": true}'; exit 2"#);
        assert!(!block(hyperlume::probe(&path, hyperlume::PROBE_TIMEOUT)));
    }

    #[test]
    fn capabilities_garbage() {
        let _g = serial();
        let d = tempfile::tempdir().unwrap();
        for out in ["not json at all", "[1, 2, 3]", r#"{"chat_plain": "yes"}"#, ""] {
            let path = fake(d.path(), &format!("echo '{out}'"));
            assert!(!block(hyperlume::probe(&path, hyperlume::PROBE_TIMEOUT)), "{out}");
        }
        // One that never answers is given up on, without waiting for it.
        let path = fake(d.path(), "exec sleep 30");
        let t = Instant::now();
        assert!(!block(hyperlume::probe(&path, Duration::from_millis(300))));
        assert!(t.elapsed() < Duration::from_secs(5));
    }

    #[test]
    fn streams_an_answer() {
        let _g = serial();
        let d = tempfile::tempdir().unwrap();
        let path = fake(
            d.path(),
            r#"echo "$@" > "$D/args"
cat > "$D/stdin.json"
echo '{"d": "Hel"}'
echo '{"d": "lo, "}'
echo 'a stray line'
echo '{"d": "world ✓"}'
echo '{"done": true}'"#,
        );
        let (chunks, r) = run(&path, "", "");
        assert_eq!(r, Ok(()));
        assert_eq!(chunks.concat(), "Hello, world ✓");
        // Nothing extra on the command line when the person picked nothing.
        assert_eq!(std::fs::read_to_string(d.path().join("args")).unwrap().trim(), "chat --plain");
        let sent: serde_json::Value = serde_json::from_slice(&std::fs::read(d.path().join("stdin.json")).unwrap()).unwrap();
        assert_eq!(
            sent,
            serde_json::json!([{"role": "system", "content": "Be brief."}, {"role": "user", "content": "Hi"}])
        );

        // The provider and model, only when picked.
        let (_, r) = run(&path, "openai", "gpt-4o-mini");
        assert_eq!(r, Ok(()));
        assert_eq!(
            std::fs::read_to_string(d.path().join("args")).unwrap().trim(),
            "chat --plain --provider openai --model gpt-4o-mini"
        );
        assert_eq!(hyperlume::args("local", ""), ["chat", "--plain", "--provider", "local"]);
    }

    #[test]
    fn each_error_exit() {
        let _g = serial();
        let d = tempfile::tempdir().unwrap();
        for code in 1..=4 {
            // Hyperlume's own sentence is what the person sees.
            let path = fake(d.path(), &format!(r#"cat > /dev/null; echo '{{"d": "par"}}'; echo '{{"error": "Sentence {code}."}}'; exit {code}"#));
            let (_, r) = run(&path, "", "");
            assert_eq!(r, Err(format!("Sentence {code}.")));
            // With no sentence, the exit code says what's wrong.
            let path = fake(d.path(), &format!("cat > /dev/null; exit {code}"));
            let (_, r) = run(&path, "", "");
            let hint = hyperlume::exit_hint(Some(code));
            assert!(!hint.is_empty());
            assert_eq!(r, Err(hint));
        }
        let hints: std::collections::HashSet<_> = (1..=4).map(|c| hyperlume::exit_hint(Some(c))).collect();
        assert_eq!(hints.len(), 4, "each exit code has its own hint");
        assert!(hyperlume::exit_hint(Some(2)).contains("Settings → AI"));
        assert!(hyperlume::exit_hint(Some(3)).contains("key"));
        // An error line wins even with exit 0, and a stream that just stops is an error.
        let path = fake(d.path(), r#"echo '{"error": "No answer."}'"#);
        assert_eq!(run(&path, "", "").1, Err("No answer.".into()));
        let path = fake(d.path(), r#"echo '{"d": "half"}'"#);
        assert!(run(&path, "", "").1.is_err());
        // Not installed.
        let empty = tempfile::tempdir().unwrap();
        let r = run(&path_with(empty.path()), "", "").1;
        assert!(r.unwrap_err().contains("isn't installed"));
    }

    #[test]
    fn cancel_sends_sigterm() {
        let _g = serial();
        let d = tempfile::tempdir().unwrap();
        let path = fake(
            d.path(),
            r#"trap 'echo term > "$D/got-term"; echo '"'"'{"d": "late"}'"'"'; exit 143' TERM
echo '{"d": "partial"}'
while :; do sleep 0.05; done"#,
        );
        let cancel = Arc::new(Cancel::default());
        let (tx, rx) = std::sync::mpsc::channel::<String>();
        let c2 = cancel.clone();
        let p2 = path.clone();
        let task = tauri::async_runtime::spawn(async move {
            let msgs = ask("Hi");
            hyperlume::chat(&p2, "", "", "", &msgs, &c2, |t| {
                let _ = tx.send(t);
            })
            .await
        });
        assert_eq!(rx.recv_timeout(Duration::from_secs(5)).unwrap(), "partial");
        let t = Instant::now();
        cancel.cancel();
        assert!(cancel.is_cancelled());
        let r = block(async { tokio::time::timeout(Duration::from_secs(5), task).await });
        assert!(r.is_ok(), "the chat ends soon after cancel");
        assert!(t.elapsed() < Duration::from_secs(4));
        assert!(d.path().join("got-term").exists(), "the program got SIGTERM");
        assert!(rx.try_iter().all(|t| t != "late"), "no text after cancel");
        // Cancelling before the program has started stops it straight away.
        let early = Cancel::default();
        early.cancel();
        let t = Instant::now();
        let _ = block(hyperlume::chat(&path, "", "", "", &ask("Hi"), &early, |_| panic!("no text after cancel")));
        assert!(t.elapsed() < Duration::from_secs(4));
    }
    #[test]
    fn invalid_utf8_is_one_error_and_no_hang() {
        let _g = serial();
        let d = tempfile::tempdir().unwrap();
        // A bad line, then far more output than a pipe holds, forever.
        let path = fake(
            d.path(),
            r#"echo '{"d": "a"}'
printf '{"d": "\377"}\n'
while :; do echo '{"d": "more"}'; done"#,
        );
        let ev = events(&path);
        assert_eq!(errors(&ev).len(), 1, "{:?}", &ev[ev.len().saturating_sub(3)..]);
        assert!(errors(&ev)[0].contains("couldn't read"));
        assert!(!ev.iter().any(|(k, _)| *k == "done"));
        assert_eq!(ev.last().unwrap().0, "error");
    }

    #[test]
    fn oversized_line_is_one_error() {
        let _g = serial();
        let d = tempfile::tempdir().unwrap();
        let path = fake(
            d.path(),
            r#"printf '{"d": "'; head -c 3000000 /dev/zero | tr '\0' a; echo '"}'
echo '{"done": true}'"#,
        );
        let ev = events(&path);
        assert_eq!(errors(&ev).len(), 1);
        assert!(errors(&ev)[0].contains("too long"));
        assert!(!ev.iter().any(|(k, _)| *k == "done"));
        assert!(!ev.iter().any(|(k, _)| *k == "chunk"), "the long line isn't passed on");
    }

    /// Output that yields `first`, then fails.
    struct Failing(Option<&'static [u8]>);

    impl tokio::io::AsyncRead for Failing {
        fn poll_read(
            mut self: std::pin::Pin<&mut Self>,
            _: &mut std::task::Context<'_>,
            buf: &mut tokio::io::ReadBuf<'_>,
        ) -> std::task::Poll<std::io::Result<()>> {
            std::task::Poll::Ready(match self.0.take() {
                Some(b) => {
                    buf.put_slice(b);
                    Ok(())
                }
                None => Err(std::io::Error::other("the pipe broke")),
            })
        }
    }

    #[test]
    fn read_failure_mid_stream_is_an_error() {
        let _g = serial();
        let mut chunks = Vec::new();
        let cancel = Cancel::default();
        let failing = tokio::io::BufReader::new(Failing(Some(b"{\"d\": \"a\"}\n{\"d\": \"b")));
        let r = block(async { tokio::time::timeout(Duration::from_secs(5), hyperlume::read_answer(failing, &cancel, |t| chunks.push(t))).await })
            .expect("no hang");
        assert_eq!(chunks, ["a"]);
        match r {
            hyperlume::Read::Broken(e) => assert!(e.contains("couldn't read"), "{e}"),
            other => panic!("expected an error, got {other:?}"),
        }
        // And an ending without a newline still counts.
        let r = block(hyperlume::read_answer(&b"{\"d\": \"x\"}\n{\"done\": true}"[..], &cancel, |_| {}));
        assert!(matches!(r, hyperlume::Read::End { done: true, error: None }), "{r:?}");
    }

    #[test]
    fn test_button_time_limit_sends_sigterm_then_kills() {
        let _g = serial();
        let d = tempfile::tempdir().unwrap();
        let msgs = ask("Hi");
        // Stops on SIGTERM.
        let path = fake(
            d.path(),
            r#"echo $$ > "$D/pid"
trap 'echo term > "$D/got-term"; exit 143' TERM
while :; do sleep 0.05; done"#,
        );
        let t = Instant::now();
        let r = block(hyperlume::ask_with_limit(&path, "", "", &msgs, Duration::from_millis(300)));
        assert!(r.unwrap_err().contains("too long"));
        assert!(d.path().join("got-term").exists(), "SIGTERM, not SIGKILL");
        assert!(t.elapsed() < Duration::from_secs(3));
        let pid = std::fs::read_to_string(d.path().join("pid")).unwrap();
        assert!(!Path::new(&format!("/proc/{}", pid.trim())).exists(), "the program was reaped");

        // Ignores SIGTERM: killed after the grace period, and reaped.
        let path = fake(
            d.path(),
            r#"echo $$ > "$D/pid"
trap '' TERM
while :; do sleep 0.05; done"#,
        );
        let t = Instant::now();
        let r = block(hyperlume::ask_with_limit(&path, "", "", &msgs, Duration::from_millis(300)));
        assert!(r.unwrap_err().contains("too long"));
        let took = t.elapsed();
        assert!(took >= Duration::from_secs(2), "a grace period first ({took:?})");
        assert!(took < Duration::from_secs(8), "then a kill ({took:?})");
        let pid = std::fs::read_to_string(d.path().join("pid")).unwrap();
        assert!(!Path::new(&format!("/proc/{}", pid.trim())).exists(), "the program was reaped");

        // In time: the answer.
        let path = fake(d.path(), r#"cat > /dev/null; echo '{"d": "ready"}'; echo '{"done": true}'"#);
        assert_eq!(block(hyperlume::ask_with_limit(&path, "", "", &msgs, Duration::from_secs(10))), Ok("ready".into()));
    }
}

/// Elsewhere (Windows, macOS, Android, iOS) Hyperlume AI is never offered.
#[cfg(all(test, not(target_os = "linux")))]
mod hyperlume_elsewhere {
    #[test]
    fn never_available() {
        assert!(!tauri::async_runtime::block_on(super::ai_hyperlume_available()));
    }
}
