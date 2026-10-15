//! "Ask my notes" and search by meaning: finds the note chunks that best
//! match a question. Word matching (BM25) always works; when an embedding
//! model is set up (Settings → AI), meaning search joins in, and the two
//! rankings are merged.

use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::Duration;

use marrow_core::engine::recall::{bm25, Chunk};
use marrow_core::vectors::VectorStore;
use serde::Serialize;
use serde_json::{json, Value};
use tauri::{AppHandle, Emitter, State};

use crate::ai::{self, EmbedSetting, Provider};
use crate::commands::{err, vault_cache_file, with_engine, AppState, CmdResult};

/// Chunks embedded per request.
const BATCH: usize = 16;
/// At question time, at most this many new or changed chunks are embedded
/// before answering; more than that waits for a full build.
const QUICK_UPDATE: usize = 48;

static BUILDING: AtomicBool = AtomicBool::new(false);

/// The loaded index: (file, store).
fn cache() -> &'static Mutex<Option<(PathBuf, VectorStore)>> {
    static C: std::sync::OnceLock<Mutex<Option<(PathBuf, VectorStore)>>> = std::sync::OnceLock::new();
    C.get_or_init(|| Mutex::new(None))
}

fn index_file(app: &AppHandle, state: &State<AppState>) -> CmdResult<PathBuf> {
    let root = with_engine(state, |e| Ok(e.vault().root().to_path_buf()))?;
    vault_cache_file(app, &root, "vectors")
}

fn model_key(s: &EmbedSetting) -> String {
    format!("{}/{}", s.provider_id, s.model)
}

/// The store for this vault and model (loaded once, then kept).
fn with_store<T>(file: &PathBuf, model: &str, f: impl FnOnce(&mut VectorStore) -> T) -> T {
    let mut guard = cache().lock().unwrap_or_else(|p| p.into_inner());
    let fresh = match guard.as_ref() {
        Some((p, s)) => p != file || s.model != model,
        None => true,
    };
    if fresh {
        let loaded = VectorStore::load(file).filter(|s| s.model == model).unwrap_or_else(|| VectorStore::new(model));
        *guard = Some((file.clone(), loaded));
    }
    f(&mut guard.as_mut().unwrap().1)
}

fn embed_provider(app: &AppHandle) -> Option<(EmbedSetting, Provider)> {
    let cfg = ai::load(app);
    let e = cfg.embed.clone().filter(|e| !e.model.trim().is_empty())?;
    let p = cfg.providers.into_iter().find(|p| p.id == e.provider_id)?;
    Some((e, p))
}

/// Embeds `texts` with the provider's OpenAI-style /embeddings endpoint.
async fn embed(app: &AppHandle, p: &Provider, model: &str, texts: &[String]) -> Result<Vec<Vec<f32>>, String> {
    if p.protocol == "anthropic" {
        return Err(format!("{} doesn't offer a model for search by meaning. Pick another provider (Ollama, OpenAI, Gemini, Mistral, Qwen…).", p.name));
    }
    let key = ai::key_for(app, p, None);
    let mut req = ai::client()?
        .post(ai::url(&p.base_url, "embeddings"))
        .timeout(Duration::from_secs(120))
        .json(&json!({ "model": model, "input": texts }));
    if let Some(k) = &key {
        req = req.bearer_auth(k);
    }
    let resp = req.send().await.map_err(|e| ai::friendly(e, p))?;
    if !resp.status().is_success() {
        return Err(ai::failure(resp).await);
    }
    let v: Value = resp.json().await.map_err(err)?;
    let data = v.get("data").and_then(|d| d.as_array()).ok_or("The provider's answer had no embeddings in it.")?;
    let mut out: Vec<(usize, Vec<f32>)> = data
        .iter()
        .enumerate()
        .map(|(i, d)| {
            let idx = d.get("index").and_then(|x| x.as_u64()).map(|x| x as usize).unwrap_or(i);
            let vec = d
                .get("embedding")
                .and_then(|e| e.as_array())
                .map(|a| a.iter().filter_map(|x| x.as_f64()).map(|x| x as f32).collect())
                .unwrap_or_default();
            (idx, vec)
        })
        .collect();
    out.sort_by_key(|x| x.0);
    if out.len() != texts.len() {
        return Err("The provider returned a different number of embeddings than asked for.".into());
    }
    Ok(out.into_iter().map(|x| x.1).collect())
}

/// What gets embedded for a chunk: where it is, then the text.
fn embed_text(c: &Chunk) -> String {
    let head = if c.heading.is_empty() { c.name.clone() } else { format!("{} › {}", c.name, c.heading) };
    let mut t = format!("{head}\n\n{}", c.text);
    if t.len() > 6000 {
        let mut cut = 6000;
        while !t.is_char_boundary(cut) {
            cut -= 1;
        }
        t.truncate(cut);
    }
    t
}

/// Embeds the chunks the store doesn't have yet (all of them, or up to `limit`).
async fn update(app: &AppHandle, file: &PathBuf, set: &EmbedSetting, p: &Provider, chunks: &[Chunk], limit: Option<usize>, progress: bool) -> Result<usize, String> {
    let model = model_key(set);
    let missing: Vec<&Chunk> = with_store(file, &model, |s| {
        let mut seen = HashSet::new();
        chunks.iter().filter(|c| !s.has(c.hash) && seen.insert(c.hash)).collect()
    });
    if let Some(l) = limit {
        if missing.len() > l {
            return Ok(missing.len());
        }
    }
    let total = missing.len();
    let mut done = 0;
    for batch in missing.chunks(BATCH) {
        let texts: Vec<String> = batch.iter().map(|c| embed_text(c)).collect();
        let vecs = embed(app, p, &set.model, &texts).await?;
        with_store(file, &model, |s| {
            for (c, v) in batch.iter().zip(vecs) {
                s.insert(c.hash, v);
            }
        });
        done += batch.len();
        if progress {
            let _ = app.emit("ai-index", json!({ "done": done, "total": total }));
            if done % (BATCH * 8) == 0 {
                with_store(file, &model, |s| s.save(file)).map_err(err)?;
            }
        }
    }
    with_store(file, &model, |s| {
        let live: HashSet<u64> = chunks.iter().map(|c| c.hash).collect();
        s.retain(&live);
        s.save(file)
    })
    .map_err(err)?;
    Ok(0)
}

// ------------------------------------------------------------- commands

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IndexInfo {
    pub provider_id: Option<String>,
    pub model: Option<String>,
    pub chunks: usize,
    pub indexed: usize,
    pub building: bool,
}

#[tauri::command]
pub fn ai_index_info(app: AppHandle, state: State<AppState>) -> CmdResult<IndexInfo> {
    let chunks = with_engine(&state, |e| Ok(e.chunks()))?;
    let set = ai::load(&app).embed;
    let indexed = match &set {
        Some(s) if !s.model.is_empty() => {
            let file = index_file(&app, &state)?;
            with_store(&file, &model_key(s), |st| chunks.iter().filter(|c| st.has(c.hash)).count())
        }
        _ => 0,
    };
    Ok(IndexInfo {
        provider_id: set.as_ref().map(|s| s.provider_id.clone()),
        model: set.as_ref().map(|s| s.model.clone()),
        chunks: chunks.len(),
        indexed,
        building: BUILDING.load(Ordering::SeqCst),
    })
}

/// Sets (or clears) the model for search by meaning.
#[tauri::command]
pub fn ai_set_embedding(app: AppHandle, provider_id: Option<String>, model: Option<String>) -> CmdResult<()> {
    let mut cfg = ai::load(&app);
    cfg.embed = match (provider_id, model) {
        (Some(p), Some(m)) if !p.is_empty() && !m.trim().is_empty() => Some(EmbedSetting { provider_id: p, model: m.trim().to_string() }),
        _ => None,
    };
    ai::save(&app, &cfg)
}

/// Embeds every chunk that isn't yet; progress arrives as `ai-index` events.
#[tauri::command]
pub async fn ai_index_build(app: AppHandle, state: State<'_, AppState>) -> CmdResult<()> {
    let Some((set, p)) = embed_provider(&app) else {
        return Err("Choose a model for search by meaning first.".into());
    };
    if BUILDING.swap(true, Ordering::SeqCst) {
        return Ok(());
    }
    let result = async {
        let chunks = with_engine(&state, |e| Ok(e.chunks()))?;
        let file = index_file(&app, &state)?;
        update(&app, &file, &set, &p, &chunks, None, true).await
    }
    .await;
    BUILDING.store(false, Ordering::SeqCst);
    match result {
        Ok(_) => {
            let _ = app.emit("ai-index", json!({ "finished": true }));
            Ok(())
        }
        Err(e) => {
            let _ = app.emit("ai-index", json!({ "error": e }));
            Err(e)
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RecallHit {
    pub path: String,
    pub name: String,
    pub line: u32,
    pub heading: String,
    pub text: String,
    /// "words", "meaning" or "both".
    pub how: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Recall {
    pub hits: Vec<RecallHit>,
    /// Whether meaning search took part.
    pub meaning: bool,
    /// Why it didn't, if it's set up but couldn't (index building, model offline…).
    pub note: Option<String>,
}

/// The chunks that best match `query`. `mode`: "both" (default) or "meaning".
#[tauri::command]
pub async fn ai_recall(app: AppHandle, state: State<'_, AppState>, query: String, k: Option<usize>, mode: Option<String>) -> CmdResult<Recall> {
    let k = k.unwrap_or(8).clamp(1, 40);
    let meaning_only = mode.as_deref() == Some("meaning");
    let chunks = with_engine(&state, |e| Ok(e.chunks()))?;
    let words = if meaning_only { Vec::new() } else { bm25(&chunks, &query, k * 3) };

    let mut sem: Vec<(usize, f32)> = Vec::new();
    let mut note = None;
    if let Some((set, p)) = embed_provider(&app) {
        let file = index_file(&app, &state)?;
        // Small changes since the last build are embedded now; big ones wait.
        let pending = if BUILDING.load(Ordering::SeqCst) {
            usize::MAX
        } else {
            match update(&app, &file, &set, &p, &chunks, Some(QUICK_UPDATE), false).await {
                Ok(n) => n,
                Err(e) => {
                    note = Some(e);
                    0
                }
            }
        };
        if pending > 0 && note.is_none() {
            note = Some(if pending == usize::MAX {
                "The meaning index is still being built; these results use words only for notes not indexed yet.".into()
            } else {
                format!("{pending} parts of your notes aren't in the meaning index yet. Build it in Settings → AI.")
            });
        }
        if note.as_deref().is_none_or(|n| !n.contains("doesn't offer") && !n.contains("Couldn't reach")) {
            match embed(&app, &p, &set.model, std::slice::from_ref(&query)).await {
                Ok(mut q) => {
                    let hashes: Vec<u64> = chunks.iter().map(|c| c.hash).collect();
                    let q = q.pop().unwrap_or_default();
                    sem = with_store(&file, &model_key(&set), |s| s.search(&hashes, &q, k * 3));
                }
                Err(e) => note = Some(e),
            }
        }
    } else if meaning_only {
        return Err("Search by meaning needs a model. Set one up in Settings → AI.".into());
    }

    // Reciprocal rank fusion of the two lists.
    let mut score: std::collections::HashMap<usize, (f32, bool, bool)> = std::collections::HashMap::new();
    for (rank, (i, _)) in words.iter().enumerate() {
        let e = score.entry(*i).or_insert((0.0, false, false));
        e.0 += 1.0 / (60.0 + rank as f32);
        e.1 = true;
    }
    for (rank, (i, s)) in sem.iter().enumerate() {
        // Weak meaning matches don't count.
        if *s < 0.2 {
            continue;
        }
        let e = score.entry(*i).or_insert((0.0, false, false));
        e.0 += 1.0 / (60.0 + rank as f32);
        e.2 = true;
    }
    let mut ranked: Vec<(usize, (f32, bool, bool))> = score.into_iter().collect();
    ranked.sort_by(|a, b| b.1 .0.total_cmp(&a.1 .0));
    ranked.truncate(k);
    let hits = ranked
        .into_iter()
        .map(|(i, (_, w, m))| {
            let c = &chunks[i];
            RecallHit {
                path: c.path.to_string(),
                name: c.name.clone(),
                line: c.line,
                heading: c.heading.clone(),
                text: c.text.clone(),
                how: match (w, m) {
                    (true, true) => "both",
                    (false, true) => "meaning",
                    _ => "words",
                }
                .into(),
            }
        })
        .collect();
    Ok(Recall { hits, meaning: !sem.is_empty(), note })
}
