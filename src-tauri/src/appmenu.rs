//! Marrow's menus in the desktop's global menu bar (Linux, Wayland only).
//!
//! The window keeps no menu bar of its own. When the desktop has a global menu
//! (someone owns `com.canonical.AppMenu.Registrar` on the session bus) and the
//! compositor speaks KDE's appmenu protocol, Marrow serves its menu on D-Bus
//! from this process and tells the compositor, through GTK's own Wayland
//! connection, which menu belongs to the window. Anywhere else this does
//! nothing. The menu itself, the D-Bus side and the protocol live in
//! `crates/marrow-appmenu`; this is the glue to Tauri, GTK and the window.
//!
//! A click on an item reaches the window as an `app-menu` event naming the
//! action (`ui/src/appmenu.ts` runs it the way its shortcut or button would),
//! except Cut, Copy and Paste, which WebKit does itself on whatever has the
//! focus. The window tells the menu its labels (translated), which toggles are
//! on and what can be used, through `app_menu_sync`. Quit goes through the
//! window too (it saves first), then `quit_app`.

use std::cell::RefCell;
use std::collections::HashMap;
use std::ffi::c_void;
use std::rc::Rc;
use std::sync::{Arc, OnceLock};

use marrow_appmenu::wayland::Appmenu;
use marrow_appmenu::{
    registrar_present, wait_for_registrar, Action, MenuServer, SharedMenu, OBJECT_PATH,
};
use tauri::{AppHandle, Emitter, Manager, WebviewWindow};

/// The menu and, once served, its D-Bus object.
#[derive(Default)]
pub struct AppMenu {
    menu: SharedMenu,
    server: OnceLock<MenuServer>,
}

/// Starts serving the menu of the main window, if the desktop can show it.
pub fn start(window: &WebviewWindow) {
    let app = window.app_handle().clone();
    app.manage(AppMenu::default());
    if !on_wayland(window) {
        log::debug!("global menu: not a Wayland session");
        return;
    }
    tauri::async_runtime::spawn(async move {
        if let Err(e) = serve(app).await {
            log::debug!("global menu: {e}");
        }
    });
}

/// Whether GTK draws the window through Wayland (not X11 or XWayland).
fn on_wayland(window: &WebviewWindow) -> bool {
    use gtk::glib::translate::FromGlib;
    use gtk::prelude::*;
    let Ok(win) = window.gtk_window() else {
        return false;
    };
    // Safety: a plain type lookup.
    let wayland =
        unsafe { gtk::glib::Type::from_glib(gdk_wayland_sys::gdk_wayland_display_get_type()) };
    WidgetExt::display(&win).type_().is_a(wayland)
}

async fn serve(app: AppHandle) -> marrow_appmenu::zbus::Result<()> {
    let conn = marrow_appmenu::zbus::Connection::session().await?;
    if !registrar_present(&conn).await {
        log::debug!("global menu: none on this desktop (yet)");
    }
    // A desktop that turns its global menu on later gets the menu then.
    wait_for_registrar(&conn).await?;
    let state = app.state::<AppMenu>();
    let on_action = {
        let app = app.clone();
        Arc::new(move |a: Action| run(&app, a))
    };
    let server = MenuServer::serve(&conn, state.menu.clone(), on_action).await?;
    let Some(name) = server.bus_name() else {
        return Err(marrow_appmenu::zbus::Error::Failure(
            "no unique bus name".into(),
        ));
    };
    let _ = state.server.set(server);
    let main = app.clone();
    app.run_on_main_thread(move || attach(&main, name))
        .map_err(|e| marrow_appmenu::zbus::Error::Failure(e.to_string()))?;
    Ok(())
}

/// A click in the global menu.
fn run(app: &AppHandle, action: Action) {
    let Some(w) = app.get_webview_window("main") else {
        return;
    };
    if let Some(command) = action.webview_command() {
        let _ = w.with_webview(move |view| {
            use webkit2gtk::WebViewExt;
            view.inner().execute_editing_command(command);
        });
        return;
    }
    let _ = w.emit("app-menu", action.name());
}

/// Points the compositor at the menu, now and each time the window is shown
/// again (GTK gives it a new surface then). On GTK's main thread.
fn attach(app: &AppHandle, service: String) {
    use gtk::prelude::*;
    let Some(w) = app.get_webview_window("main") else {
        return;
    };
    let Ok(win) = w.gtk_window() else { return };
    let display = WidgetExt::display(&win);
    // Safety: `display` is GTK's Wayland display (checked in `on_wayland`),
    // alive for as long as the app, and this is GTK's main thread.
    let appmenu = unsafe {
        let wl = gdk_wayland_sys::gdk_wayland_display_get_wl_display(display.as_ptr().cast());
        Appmenu::new(wl.cast::<c_void>())
    };
    let Some(appmenu) = appmenu else { return };
    let appmenu = Rc::new(RefCell::new(appmenu));
    let set = move |win: &gtk::ApplicationWindow| {
        let Some(gdk_window) = win.window() else {
            return;
        };
        // Safety: the window's own GdkWindow on the Wayland display; its
        // surface stays alive while the window is shown.
        let surface = unsafe {
            gdk_wayland_sys::gdk_wayland_window_get_wl_surface(gdk_window.as_ptr().cast())
        };
        if surface.is_null() {
            return;
        }
        // Safety: as above, on the display's thread.
        match unsafe {
            appmenu
                .borrow_mut()
                .attach(surface.cast::<c_void>(), &service, OBJECT_PATH)
        } {
            Ok(()) => log::info!("global menu: offered to the desktop"),
            Err(e) => log::debug!("global menu: {e}"),
        }
    };
    if win.is_mapped() {
        set(&win);
    }
    // The handler keeps the appmenu object alive as long as the window.
    win.connect_map(set);
}

/// The window tells the menu what to show: `labels` in its language (empty for
/// English), which toggles are `checked`, which items are `enabled`. Returns
/// each menu and item's name and English label, for the window to translate.
#[tauri::command]
pub async fn app_menu_sync(
    app: AppHandle,
    labels: HashMap<String, String>,
    checked: HashMap<String, bool>,
    enabled: HashMap<String, bool>,
) -> Vec<(String, String)> {
    let Some(state) = app.try_state::<AppMenu>() else {
        return Vec::new();
    };
    let (changed, english) = {
        let mut m = state.menu.lock().unwrap_or_else(|e| e.into_inner());
        (m.update(labels, checked, enabled), m.english_labels())
    };
    if changed {
        if let Some(server) = state.server.get() {
            if let Err(e) = server.changed().await {
                log::debug!("global menu: {e}");
            }
        }
    }
    english
}
