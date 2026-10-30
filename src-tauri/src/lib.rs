//! Marrow's app: the Tauri shell around marrow-core, shared by the desktop
//! app (main.rs) and the iOS and Android apps.

mod ai;
mod capture;
mod clipper;
mod import;
mod extras;
mod locks;
mod commands;
mod portable;
mod publish;
mod recall;
mod reminders;
mod sync;
use marrow_webdav as webdav;

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
        .plugin(capture::plugin())
        .plugin(tauri_plugin_autostart::init(tauri_plugin_autostart::MacosLauncher::LaunchAgent, Some(vec!["--background"])))
        // With "keep running for reminders" on, closing the window hides it
        // to the tray instead of quitting.
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                if window.label() == "main" && commands::load_settings(window.app_handle()).background {
                    api.prevent_close();
                    let _ = window.hide();
                }
            }
        });
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
                // Started at sign-in: stay in the tray until opened.
                let quiet = std::env::args().any(|a| a == "--background");
                win = win.visible(!quiet);
                win.build()?;
                tray(app.handle())?;
            }
            reminders::start(app.handle());
            #[cfg(desktop)]
            clipper::start(app.handle());
            // Phones and tablets: one full-screen window.
            #[cfg(mobile)]
            WebviewWindowBuilder::new(app, "main", WebviewUrl::default()).build()?;
            Ok(())
        })
        .plugin(tauri_plugin_notification::init())
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
            import::find_exports,
            commands::history_read,
            commands::history_snapshot,
            commands::history_vault_at,
            commands::history_timeline,
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
            sync::drive_services,
            sync::drive_sign_in,
            sync::drive_sign_in_cancel,
            sync::drive_check,
            sync::set_drive_sync,
            sync::folder_encrypted,
            sync::sync_now,
            sync::conflict_copies,
            sync::resolve_conflict,
            reminders::reminder_line,
            reminders::task_text,
            reminders::reminder_of,
            reminders::background_enabled,
            reminders::set_background,
        ])
        .run(tauri::generate_context!())
        .expect("error while running Marrow");
}

/// The tray icon: open Marrow or quit it (closing the window only hides it
/// while "keep running for reminders" is on).
#[cfg(desktop)]
fn tray(app: &tauri::AppHandle) -> tauri::Result<()> {
    use tauri::menu::{Menu, MenuItem};
    use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
    let open = MenuItem::with_id(app, "open", "Open Marrow", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Quit Marrow", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&open, &quit])?;
    let show = |app: &tauri::AppHandle| {
        if let Some(w) = app.get_webview_window("main") {
            let _ = w.unminimize();
            let _ = w.show();
            let _ = w.set_focus();
        }
    };
    let mut b = TrayIconBuilder::with_id("marrow").tooltip("Marrow").menu(&menu).show_menu_on_left_click(false);
    if let Some(icon) = app.default_window_icon() {
        b = b.icon(icon.clone());
    }
    b.on_menu_event(move |app, e| match e.id().as_ref() {
        "open" => show(app),
        "quit" => app.exit(0),
        _ => {}
    })
    .on_tray_icon_event(move |tray, e| {
        if let TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. } = e {
            show(tray.app_handle());
        }
    })
    .build(app)?;
    Ok(())
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
