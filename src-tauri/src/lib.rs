//! Marrow's app: the Tauri shell around marrow-core, shared by the desktop
//! app (main.rs) and the iOS and Android apps.

mod ai;
mod capture;
mod import;
mod extras;
mod locks;
mod commands;
mod portable;
mod publish;
mod recall;
mod sync;
mod webdav;

use std::sync::Mutex;

use commands::AppState;
#[cfg(desktop)]
use tauri::Manager;
use tauri::{WebviewUrl, WebviewWindowBuilder};

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    portable::prepare();
    let builder = tauri::Builder::default();
    // Opening Marrow again brings the open window forward instead of
    // starting a second copy that can't use the vault.
    #[cfg(desktop)]
    let builder = builder
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            if let Some(w) = app.get_webview_window("main") {
                let _ = w.unminimize();
                let _ = w.show();
                let _ = w.set_focus();
            }
        }))
        .plugin(capture::plugin());
    builder
        .plugin(log_plugin())
        .setup(|app| {
            // A crash leaves its reason in the log, for a bug report.
            let default_hook = std::panic::take_hook();
            std::panic::set_hook(Box::new(move |info| {
                log::error!("crash: {info}");
                default_hook(info);
            }));
            log::info!("Marrow {} started on {} {}", env!("CARGO_PKG_VERSION"), std::env::consts::OS, std::env::consts::ARCH);
            #[cfg(desktop)]
            {
                let mut win = WebviewWindowBuilder::new(app, "main", WebviewUrl::default())
                    .title("Marrow")
                    .inner_size(1280.0, 820.0)
                    .min_inner_size(760.0, 480.0)
                    .disable_drag_drop_handler();
                // Portable: the window's own storage (tabs, preferences) stays beside the program.
                if let Some(dir) = portable::data("webview") {
                    win = win.data_directory(dir);
                }
                win.build()?;
            }
            // Phones and tablets: one full-screen window.
            #[cfg(mobile)]
            WebviewWindowBuilder::new(app, "main", WebviewUrl::default()).build()?;
            Ok(())
        })
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .manage(locks::Locks::default())
        .manage(AppState {
            service: Mutex::new(None),
            mirror: Mutex::new(None),
        })
        .invoke_handler(tauri::generate_handler![
            commands::last_vault,
            commands::app_platform,
            commands::default_vault,
            commands::open_vault,
            commands::tree,
            commands::read_note,
            locks::lock_info,
            locks::locked_paths,
            locks::unlock,
            locks::lock_note,
            locks::lock_folder,
            locks::remove_lock,
            locks::lock_now,
            publish::site_begin,
            publish::site_write,
            publish::site_copy,
            publish::site_finish,
            publish::site_open,
            extras::extras_dir,
            extras::snippets_list,
            extras::plugins_list,
            extras::extras_write,
            commands::write_note,
            commands::backup_for,
            commands::discard_backup,
            commands::keep_draft,
            extras::open_log_folder,
            commands::merge_text,
            commands::create_note,
            commands::create_folder,
            commands::trash,
            commands::rename,
            commands::move_path,
            commands::graph,
            commands::note_info,
            commands::vault_stats,
            commands::recent_files,
            commands::import_file,
            commands::save_attachment,
            commands::save_clipboard_image,
            commands::write_export,
            commands::create_file,
            commands::get_vault_settings,
            commands::set_vault_settings,
            commands::search,
            commands::unlinked_mentions,
            commands::history_list,
            commands::tasks,
            capture::set_capture_shortcut,
            recall::ai_index_info,
            recall::ai_set_embedding,
            recall::ai_index_build,
            recall::ai_recall,
            import::import_files,
            commands::history_read,
            commands::history_snapshot,
            commands::tags,
            commands::backlinks,
            commands::note_links,
            commands::resolve_link,
            commands::follow_link,
            commands::find_files,
            commands::query_notes,
            ai::ai_config,
            ai::ai_save_provider,
            ai::ai_remove_provider,
            ai::ai_set_default,
            ai::ai_models,
            ai::ai_test,
            ai::ai_run,
            ai::ai_cancel,
            ai::ai_transcriber,
            ai::ai_transcribe,
            sync::cloud_locations,
            sync::create_vault,
            sync::vault_cloud,
            sync::mirror_status,
            sync::mirror_preview,
            sync::set_mirror,
            sync::webdav_check,
            sync::set_cloud_sync,
            sync::folder_encrypted,
            sync::sync_now,
            sync::conflict_copies,
            sync::resolve_conflict,
        ])
        .run(tauri::generate_context!())
        .expect("error while running Marrow");
}

/// Diagnostic log: what Marrow did and what went wrong, for bug reports.
/// It never contains note text or passwords. Kept to a few small files in
/// the app's log folder (or `Data\logs` in the portable version).
fn log_plugin() -> tauri::plugin::TauriPlugin<tauri::Wry> {
    use tauri_plugin_log::{RotationStrategy, Target, TargetKind};
    let file = match portable::data("logs") {
        Some(dir) => TargetKind::Folder { path: dir, file_name: Some("marrow".into()) },
        None => TargetKind::LogDir { file_name: Some("marrow".into()) },
    };
    tauri_plugin_log::Builder::new()
        .targets([Target::new(file), Target::new(TargetKind::Stdout)])
        .level(log::LevelFilter::Info)
        .max_file_size(1_000_000)
        .rotation_strategy(RotationStrategy::KeepSome(3))
        .build()
}
