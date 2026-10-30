//! Task reminders on computers (`- [ ] Call Sam ⏰ 2026-11-03 09:00`).
//!
//! A thread checks the open vault's reminders every 20 seconds and, when one
//! is due, shows a system notification and tells the window (which shows it
//! with Done, Snooze and Open). Reminders only arrive while Marrow runs, so
//! it can keep running in the tray and start when the person signs in.
//! Reminders missed while Marrow was closed arrive when it opens, if they're
//! less than a day old. Each one rings once (remembered in `reminders.json`).

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::time::Duration;

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State};

use crate::commands::{err, with_engine, AppState, CmdResult};

const EVERY: Duration = Duration::from_secs(20);
/// Missed reminders older than this don't ring any more.
const MISSED_HOURS: i64 = 24;

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Due {
    pub id: String,
    pub path: String,
    pub name: String,
    pub line: u32,
    pub text: String,
    pub at: String,
}

fn now_local() -> chrono::NaiveDateTime {
    chrono::Local::now().naive_local()
}

fn parse(at: &str) -> Option<chrono::NaiveDateTime> {
    chrono::NaiveDateTime::parse_from_str(at, "%Y-%m-%d %H:%M").ok()
}

fn fired_file(app: &AppHandle) -> Option<PathBuf> {
    let dir = match crate::portable::data("config") {
        Some(d) => d,
        None => app.path().app_config_dir().ok()?,
    };
    Some(dir.join("reminders.json"))
}

/// Reminder id → when it rang (`YYYY-MM-DD HH:MM`).
fn load_fired(app: &AppHandle) -> BTreeMap<String, String> {
    fired_file(app).and_then(|p| std::fs::read(p).ok()).and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default()
}

fn save_fired(app: &AppHandle, fired: &BTreeMap<String, String>) {
    if let Some(p) = fired_file(app) {
        if let Some(d) = p.parent() {
            let _ = std::fs::create_dir_all(d);
        }
        let _ = marrow_core::atomic::write(&p, serde_json::to_vec(fired).unwrap_or_default());
    }
}

fn id_of(path: &str, text: &str, at: &str) -> String {
    marrow_core::cache::hash_bytes(format!("{path}\n{text}\n{at}").as_bytes())[..16].to_string()
}

/// The reminders that should ring now and haven't yet.
fn due_now(app: &AppHandle, fired: &BTreeMap<String, String>) -> Vec<Due> {
    let state: State<AppState> = app.state();
    let Ok(tasks) = with_engine(&state, |e| Ok(e.tasks(false))) else { return vec![] };
    let now = now_local();
    let oldest = now - chrono::Duration::hours(MISSED_HOURS);
    tasks
        .into_iter()
        .filter_map(|t| {
            let at = t.remind?;
            let when = parse(&at)?;
            if when > now || when < oldest {
                return None;
            }
            let id = id_of(t.path.as_str(), &t.text, &at);
            (!fired.contains_key(&id)).then(|| Due { id, path: t.path.to_string(), name: t.name, line: t.line, text: t.text, at })
        })
        .collect()
}

fn notify(app: &AppHandle, d: &Due) {
    use tauri_plugin_notification::NotificationExt;
    let _ = app.notification().builder().title(&d.text).body(&d.name).show();
}

/// Starts the thread that rings reminders.
pub fn start(app: &AppHandle) {
    let app = app.clone();
    std::thread::Builder::new()
        .name("marrow-reminders".into())
        .spawn(move || {
            let mut fired = load_fired(&app);
            loop {
                let due = due_now(&app, &fired);
                if !due.is_empty() {
                    let stamp = now_local().format("%Y-%m-%d %H:%M").to_string();
                    for d in &due {
                        log::info!("reminder due");
                        notify(&app, d);
                        let _ = app.emit("reminder", d.clone());
                        fired.insert(d.id.clone(), stamp.clone());
                    }
                    // Forget the ones from more than a week ago.
                    let week = (now_local() - chrono::Duration::days(7)).format("%Y-%m-%d %H:%M").to_string();
                    fired.retain(|_, when| *when >= week);
                    save_fired(&app, &fired);
                }
                std::thread::sleep(EVERY);
            }
        })
        .ok();
}

/// A task line with its reminder set (or removed), for the window to write
/// into the note.
#[tauri::command]
pub fn reminder_line(line: String, at: Option<String>) -> String {
    marrow_core::engine::set_remind(&line, at.as_deref())
}

/// A task line's text as the Tasks view shows it, to find a task again
/// after lines have moved.
#[tauri::command]
pub fn task_text(line: String) -> Option<String> {
    marrow_core::engine::task_text_of(&line)
}

/// The reminder on a task line (`YYYY-MM-DD HH:MM`), if any.
#[tauri::command]
pub fn reminder_of(line: String) -> Option<String> {
    marrow_core::engine::remind_of(&line)
}

/// Whether Marrow keeps running in the tray and starts at sign-in, so
/// reminders arrive with the window closed.
#[tauri::command]
pub fn background_enabled(app: AppHandle) -> bool {
    crate::commands::load_settings(&app).background
}

#[tauri::command]
pub fn set_background(app: AppHandle, on: bool) -> CmdResult<()> {
    let mut s = crate::commands::load_settings(&app);
    s.background = on;
    crate::commands::save_settings(&app, &s)?;
    #[cfg(desktop)]
    {
        use tauri_plugin_autostart::ManagerExt;
        let a = app.autolaunch();
        // The portable version isn't installed, so it doesn't start itself.
        if crate::portable::data("config").is_none() {
            if on { a.enable() } else { a.disable() }.map_err(err)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn times() {
        assert_eq!(parse("2026-11-03 09:00").unwrap().to_string(), "2026-11-03 09:00:00");
        assert!(parse("tomorrow").is_none());
        assert_ne!(id_of("a.md", "x", "2026-11-03 09:00"), id_of("a.md", "x", "2026-11-03 09:10"));
    }
}
