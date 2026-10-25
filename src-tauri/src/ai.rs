//! Bring-your-own AI. Marrow has no AI of its own: the person adds a provider
//! (OpenAI, Claude, Gemini, DeepSeek, Qwen, Kimi, a local Ollama…) with their
//! own key, and Marrow sends what they ask for to that provider only.
//!
//! Almost every provider speaks the OpenAI chat-completions protocol; Claude
//! uses Anthropic's Messages API. Requests are made here, not in the window,
//! so browser rules (CORS) don't get in the way, and answers stream back as
//! `ai-event` events.
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
    /// "openai" (chat completions) or "anthropic" (messages).
    pub protocol: String,
    pub base_url: String,
    pub model: String,
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
    let key = key_for(&app, &provider, key);
    if provider.model.trim().is_empty() {
        return Err("Pick a model first.".into());
    }
    let msgs = [Msg {
        role: "user".into(),
        content: "Reply with just the word: ready".into(),
    }];
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
    if let Ok(mut r) = running().lock() {
        if let Some(t) = r.remove(&id) {
            t.abort();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_stream_deltas() {
        let o: Value = serde_json::from_str(r#"{"choices":[{"delta":{"content":"Hel"}}]}"#).unwrap();
        assert_eq!(delta("openai", &o).as_deref(), Some("Hel"));
        let a: Value = serde_json::from_str(r#"{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"lo"}}"#).unwrap();
        assert_eq!(delta("anthropic", &a).as_deref(), Some("lo"));
        let start: Value = serde_json::from_str(r#"{"type":"message_start"}"#).unwrap();
        assert_eq!(delta("anthropic", &start), None);
        let e: Value = serde_json::from_str(r#"{"error":{"message":"bad key"}}"#).unwrap();
        assert_eq!(stream_error(&e).as_deref(), Some("bad key"));
        assert_eq!(url("https://api.x.ai/v1/", "models"), "https://api.x.ai/v1/models");
    }
}
