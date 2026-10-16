// Hide the console window on Windows release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

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

use std::sync::Mutex;

use commands::AppState;
use tauri::{Manager, WebviewUrl, WebviewWindowBuilder};

fn main() {
    portable::prepare();
    tauri::Builder::default()
        // Opening Marrow again brings the open window forward instead of
        // starting a second copy that can't use the vault.
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            if let Some(w) = app.get_webview_window("main") {
                let _ = w.unminimize();
                let _ = w.show();
                let _ = w.set_focus();
            }
        }))
        .setup(|app| {
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
            Ok(())
        })
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(capture::plugin())
        .manage(locks::Locks::default())
        .manage(AppState {
            service: Mutex::new(None),
            mirror: Mutex::new(None),
        })
        .invoke_handler(tauri::generate_handler![
            commands::last_vault,
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
            sync::cloud_locations,
            sync::create_vault,
            sync::vault_cloud,
            sync::mirror_status,
            sync::mirror_preview,
            sync::set_mirror,
            sync::sync_now,
            sync::conflict_copies,
            sync::resolve_conflict,
        ])
        .run(tauri::generate_context!())
        .expect("error while running Marrow");
}
