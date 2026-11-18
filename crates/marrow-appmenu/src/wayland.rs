//! Telling the compositor where a window's menu is, with KDE's appmenu protocol
//! (`org_kde_kwin_appmenu_manager`), as Qt 6.9+ and Firefox do on Wayland.
//!
//! Marrow's window belongs to GTK, so this works on GTK's own Wayland
//! connection: [`Appmenu::new`] takes its `wl_display` and [`Appmenu::attach`]
//! the window's `wl_surface`, and both must be called on GTK's main thread.

use std::ffi::c_void;

use wayland_backend::client::{Backend, ObjectId};
use wayland_client::globals::{registry_queue_init, GlobalListContents};
use wayland_client::protocol::wl_registry::WlRegistry;
use wayland_client::protocol::wl_surface::WlSurface;
use wayland_client::{Connection, Dispatch, EventQueue, Proxy, QueueHandle};

/// The protocol's client side, generated from KDE's `appmenu.xml`.
#[allow(missing_docs)]
pub mod protocol {
    use wayland_client;
    use wayland_client::protocol::*;

    pub mod __interfaces {
        use wayland_client::protocol::__interfaces::*;
        wayland_scanner::generate_interfaces!("protocols/appmenu.xml");
    }
    use self::__interfaces::*;

    wayland_scanner::generate_client_code!("protocols/appmenu.xml");
}

use protocol::org_kde_kwin_appmenu::OrgKdeKwinAppmenu;
use protocol::org_kde_kwin_appmenu_manager::OrgKdeKwinAppmenuManager;

/// Why the menu couldn't be attached.
#[derive(Debug)]
pub enum Error {
    /// The surface pointer isn't a `wl_surface` of this display.
    Surface,
    /// The connection to the compositor failed.
    Connection(String),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::Surface => f.write_str("not a wl_surface"),
            Error::Connection(e) => write!(f, "Wayland connection: {e}"),
        }
    }
}

impl std::error::Error for Error {}

/// Nothing to handle: the protocol has no events.
struct State;

impl Dispatch<WlRegistry, GlobalListContents> for State {
    fn event(
        _: &mut State,
        _: &WlRegistry,
        _: wayland_client::protocol::wl_registry::Event,
        _: &GlobalListContents,
        _: &Connection,
        _: &QueueHandle<State>,
    ) {
    }
}

wayland_client::delegate_noop!(State: OrgKdeKwinAppmenuManager);
wayland_client::delegate_noop!(State: OrgKdeKwinAppmenu);

/// The appmenu manager on a foreign (GTK's) Wayland connection, and the menu
/// address of one window.
pub struct Appmenu {
    conn: Connection,
    queue: EventQueue<State>,
    manager: OrgKdeKwinAppmenuManager,
    menu: Option<OrgKdeKwinAppmenu>,
}

impl Appmenu {
    /// Binds the compositor's appmenu manager on `display`. `None` if the
    /// compositor has none (it doesn't show app menus).
    ///
    /// # Safety
    ///
    /// `display` must be a live `struct wl_display *` that outlives the
    /// returned value, used from the thread that dispatches it (GTK's main
    /// thread); this blocks for one round trip to the compositor.
    pub unsafe fn new(display: *mut c_void) -> Option<Appmenu> {
        if display.is_null() {
            return None;
        }
        // Safety: the caller promises a live display that outlives us.
        let backend = unsafe { Backend::from_foreign_display(display.cast()) };
        let conn = Connection::from_backend(backend);
        let (globals, queue) = match registry_queue_init::<State>(&conn) {
            Ok(g) => g,
            Err(e) => {
                log::debug!("global menu: no Wayland registry: {e}");
                return None;
            }
        };
        let manager =
            match globals.bind::<OrgKdeKwinAppmenuManager, _, _>(&queue.handle(), 1..=2, ()) {
                Ok(m) => m,
                Err(e) => {
                    log::debug!("global menu: the compositor shows no app menus ({e})");
                    return None;
                }
            };
        Some(Appmenu {
            conn,
            queue,
            manager,
            menu: None,
        })
    }

    /// Tells the compositor that the menu of the window drawn on `surface` is
    /// at `service` (a D-Bus name) and `path`. Call it again whenever the
    /// window gets a new surface (GTK makes one each time it is shown).
    ///
    /// # Safety
    ///
    /// `surface` must be a live `struct wl_surface *` of this display, used
    /// on the display's thread.
    pub unsafe fn attach(
        &mut self,
        surface: *mut c_void,
        service: &str,
        path: &str,
    ) -> Result<(), Error> {
        if surface.is_null() {
            return Err(Error::Surface);
        }
        // Safety: the caller promises a live wl_surface of this display.
        let id = unsafe { ObjectId::from_ptr(WlSurface::interface(), surface.cast()) }
            .map_err(|_| Error::Surface)?;
        let surface = WlSurface::from_id(&self.conn, id).map_err(|_| Error::Surface)?;
        self.release_menu();
        let menu = self.manager.create(&surface, &self.queue.handle(), ());
        menu.set_address(service.to_string(), path.to_string());
        self.menu = Some(menu);
        self.flush()
    }

    /// Takes the menu off the window.
    pub fn detach(&mut self) -> Result<(), Error> {
        self.release_menu();
        self.flush()
    }

    fn release_menu(&mut self) {
        if let Some(m) = self.menu.take() {
            m.release();
        }
    }

    fn flush(&mut self) -> Result<(), Error> {
        // Nothing is ever dispatched on this queue (the protocol has no
        // events); this only empties it.
        self.queue
            .dispatch_pending(&mut State)
            .map_err(|e| Error::Connection(e.to_string()))?;
        self.conn
            .flush()
            .map_err(|e| Error::Connection(e.to_string()))
    }
}

impl Drop for Appmenu {
    fn drop(&mut self) {
        self.release_menu();
        if self.manager.version() >= 2 {
            self.manager.release();
        }
        let _ = self.conn.flush();
    }
}
