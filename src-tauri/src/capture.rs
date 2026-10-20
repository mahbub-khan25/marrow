//! Quick capture: a system-wide shortcut that brings Marrow forward with a
//! small box for jotting something down, from any app.

use tauri::{AppHandle, Emitter, Manager, Runtime};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut, ShortcutState};

use crate::commands::CmdResult;

pub fn plugin<R: Runtime>() -> tauri::plugin::TauriPlugin<R> {
    tauri_plugin_global_shortcut::Builder::new()
        .with_handler(|app, _shortcut, event| {
            if event.state() != ShortcutState::Pressed {
                return;
            }
            if let Some(w) = app.get_webview_window("main") {
                // Tell the page whether Marrow was in the background, so it can
                // step back after the note is saved.
                let was_hidden = w.is_minimized().unwrap_or(false) || !w.is_focused().unwrap_or(true);
                let _ = w.unminimize();
                let _ = w.show();
                let _ = w.set_focus();
                let _ = w.emit("quick-capture", was_hidden);
            }
        })
        .build()
}

/// Sets the quick-capture shortcut ("Ctrl+Alt+N" style), or turns it off.
#[tauri::command(async)]
pub fn set_capture_shortcut(app: AppHandle, shortcut: Option<String>) -> CmdResult<()> {
    let gs = app.global_shortcut();
    gs.unregister_all().map_err(|e| e.to_string())?;
    let Some(s) = shortcut.filter(|s| !s.trim().is_empty()) else { return Ok(()) };
    let parsed: Shortcut = s.parse().map_err(|e| format!("“{s}” isn't a shortcut Marrow understands: {e}"))?;
    gs.register(parsed)
        .map_err(|e| format!("Couldn't use {s} — another app may already use it. ({e})"))
}
