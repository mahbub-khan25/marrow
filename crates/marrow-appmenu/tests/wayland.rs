//! The appmenu protocol against a small compositor of the test's own (over a
//! socket pair, no screen involved): the client side runs on a "foreign"
//! display, as it does on GTK's.

#![cfg(target_os = "linux")]

use std::ffi::c_void;
use std::os::unix::net::UnixStream;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use marrow_appmenu::wayland::Appmenu;
use wayland_client::globals::{registry_queue_init, GlobalListContents};
use wayland_client::protocol::wl_compositor::WlCompositor;
use wayland_client::protocol::wl_registry::WlRegistry;
use wayland_client::protocol::wl_surface::WlSurface;
use wayland_client::{Connection, Proxy};

pub mod server_protocol {
    use wayland_server;
    use wayland_server::protocol::*;

    pub mod __interfaces {
        use wayland_server::protocol::__interfaces::*;
        wayland_scanner::generate_interfaces!("protocols/appmenu.xml");
    }
    use self::__interfaces::*;

    wayland_scanner::generate_server_code!("protocols/appmenu.xml");
}

mod compositor {
    use super::server_protocol::org_kde_kwin_appmenu::{self, OrgKdeKwinAppmenu};
    use super::server_protocol::org_kde_kwin_appmenu_manager::{self, OrgKdeKwinAppmenuManager};
    use std::sync::{Arc, Mutex};
    use wayland_server::backend::{ClientData, ClientId, DisconnectReason};
    use wayland_server::protocol::wl_compositor::{self, WlCompositor};
    use wayland_server::protocol::wl_surface::WlSurface;
    use wayland_server::{
        Client, DataInit, Dispatch, DisplayHandle, GlobalDispatch, New, Resource,
    };

    /// What the compositor was told, in order.
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub enum Seen {
        /// A menu object for the surface with this protocol id.
        Created(u32),
        /// `set_address` for the surface with this protocol id.
        Address(u32, String, String),
        /// A menu object released (its surface's protocol id).
        Released(u32),
        ManagerReleased,
    }

    pub struct Comp {
        pub seen: Arc<Mutex<Vec<Seen>>>,
    }

    impl Comp {
        fn see(&self, s: Seen) {
            self.seen.lock().unwrap().push(s);
        }
    }

    pub struct NoData;
    impl ClientData for NoData {
        fn initialized(&self, _: ClientId) {}
        fn disconnected(&self, _: ClientId, _: DisconnectReason) {}
    }

    impl GlobalDispatch<WlCompositor, ()> for Comp {
        fn bind(
            _: &mut Comp,
            _: &DisplayHandle,
            _: &Client,
            r: New<WlCompositor>,
            _: &(),
            init: &mut DataInit<'_, Comp>,
        ) {
            init.init(r, ());
        }
    }

    impl Dispatch<WlCompositor, ()> for Comp {
        fn request(
            _: &mut Comp,
            _: &Client,
            _: &WlCompositor,
            request: wl_compositor::Request,
            _: &(),
            _: &DisplayHandle,
            init: &mut DataInit<'_, Comp>,
        ) {
            if let wl_compositor::Request::CreateSurface { id } = request {
                init.init(id, ());
            }
        }
    }

    impl Dispatch<WlSurface, ()> for Comp {
        fn request(
            _: &mut Comp,
            _: &Client,
            _: &WlSurface,
            _: wayland_server::protocol::wl_surface::Request,
            _: &(),
            _: &DisplayHandle,
            _: &mut DataInit<'_, Comp>,
        ) {
        }
    }

    impl GlobalDispatch<OrgKdeKwinAppmenuManager, ()> for Comp {
        fn bind(
            _: &mut Comp,
            _: &DisplayHandle,
            _: &Client,
            r: New<OrgKdeKwinAppmenuManager>,
            _: &(),
            init: &mut DataInit<'_, Comp>,
        ) {
            init.init(r, ());
        }
    }

    impl Dispatch<OrgKdeKwinAppmenuManager, ()> for Comp {
        fn request(
            state: &mut Comp,
            _: &Client,
            _: &OrgKdeKwinAppmenuManager,
            request: org_kde_kwin_appmenu_manager::Request,
            _: &(),
            _: &DisplayHandle,
            init: &mut DataInit<'_, Comp>,
        ) {
            match request {
                org_kde_kwin_appmenu_manager::Request::Create { id, surface } => {
                    let sid = surface.id().protocol_id();
                    init.init(id, sid);
                    state.see(Seen::Created(sid));
                }
                org_kde_kwin_appmenu_manager::Request::Release => state.see(Seen::ManagerReleased),
            }
        }
    }

    impl Dispatch<OrgKdeKwinAppmenu, u32> for Comp {
        fn request(
            state: &mut Comp,
            _: &Client,
            _: &OrgKdeKwinAppmenu,
            request: org_kde_kwin_appmenu::Request,
            sid: &u32,
            _: &DisplayHandle,
            _: &mut DataInit<'_, Comp>,
        ) {
            match request {
                org_kde_kwin_appmenu::Request::SetAddress {
                    service_name,
                    object_path,
                } => state.see(Seen::Address(*sid, service_name, object_path)),
                org_kde_kwin_appmenu::Request::Release => state.see(Seen::Released(*sid)),
            }
        }
    }
}

use compositor::{Comp, NoData, Seen};
use server_protocol::org_kde_kwin_appmenu_manager::OrgKdeKwinAppmenuManager;

/// A compositor on its own thread, with or without the appmenu manager.
struct Server {
    seen: Arc<Mutex<Vec<Seen>>>,
    stop: Arc<AtomicBool>,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl Server {
    fn start(appmenu: bool) -> (Server, UnixStream) {
        let (client, server) = UnixStream::pair().unwrap();
        let seen = Arc::new(Mutex::new(Vec::new()));
        let stop = Arc::new(AtomicBool::new(false));
        let thread = std::thread::spawn({
            let seen = seen.clone();
            let stop = stop.clone();
            move || {
                let mut display = wayland_server::Display::<Comp>::new().unwrap();
                let dh = display.handle();
                dh.create_global::<Comp, wayland_server::protocol::wl_compositor::WlCompositor, ()>(4, ());
                if appmenu {
                    dh.create_global::<Comp, OrgKdeKwinAppmenuManager, ()>(2, ());
                }
                display
                    .handle()
                    .insert_client(server, Arc::new(NoData))
                    .unwrap();
                let mut state = Comp { seen };
                while !stop.load(Ordering::Relaxed) {
                    display.dispatch_clients(&mut state).unwrap();
                    display.flush_clients().unwrap();
                    std::thread::sleep(Duration::from_millis(1));
                }
            }
        });
        (
            Server {
                seen,
                stop,
                thread: Some(thread),
            },
            client,
        )
    }

    /// Waits until the compositor has seen `n` things, and returns them.
    fn wait_for(&self, n: usize) -> Vec<Seen> {
        let until = Instant::now() + Duration::from_secs(5);
        loop {
            let seen = self.seen.lock().unwrap().clone();
            if seen.len() >= n || Instant::now() > until {
                return seen;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

/// The toolkit's side (GTK, in Marrow): its own connection and a window surface.
struct Toolkit;

impl wayland_client::Dispatch<WlRegistry, GlobalListContents> for Toolkit {
    fn event(
        _: &mut Toolkit,
        _: &WlRegistry,
        _: wayland_client::protocol::wl_registry::Event,
        _: &GlobalListContents,
        _: &Connection,
        _: &wayland_client::QueueHandle<Toolkit>,
    ) {
    }
}
wayland_client::delegate_noop!(Toolkit: WlCompositor);
wayland_client::delegate_noop!(Toolkit: ignore WlSurface);

fn toolkit(
    socket: UnixStream,
) -> (
    Connection,
    WlCompositor,
    wayland_client::EventQueue<Toolkit>,
) {
    let conn = Connection::from_socket(socket).unwrap();
    let (globals, queue) = registry_queue_init::<Toolkit>(&conn).unwrap();
    let compositor: WlCompositor = globals.bind(&queue.handle(), 1..=4, ()).unwrap();
    (conn, compositor, queue)
}

fn ptr<P: Proxy>(p: &P) -> *mut c_void {
    p.id().as_ptr().cast()
}

#[test]
fn the_window_gets_its_menu_address() {
    let (server, socket) = Server::start(true);
    let (conn, compositor, mut queue) = toolkit(socket);
    let surface: WlSurface = compositor.create_surface(&queue.handle(), ());
    queue.roundtrip(&mut Toolkit).unwrap();
    let display = conn.backend().display_ptr().cast::<c_void>();

    let mut menu =
        unsafe { Appmenu::new(display) }.expect("the compositor has the appmenu manager");
    unsafe { menu.attach(ptr(&surface), ":1.42", "/MenuBar") }.unwrap();
    let sid = surface.id().protocol_id();
    assert_eq!(
        server.wait_for(2),
        [
            Seen::Created(sid),
            Seen::Address(sid, ":1.42".into(), "/MenuBar".into())
        ]
    );

    // The window is shown again on a new surface: the old menu object goes.
    let again: WlSurface = compositor.create_surface(&queue.handle(), ());
    queue.roundtrip(&mut Toolkit).unwrap();
    unsafe { menu.attach(ptr(&again), ":1.42", "/MenuBar") }.unwrap();
    let sid2 = again.id().protocol_id();
    assert_eq!(
        server.wait_for(5)[2..],
        [
            Seen::Released(sid),
            Seen::Created(sid2),
            Seen::Address(sid2, ":1.42".into(), "/MenuBar".into())
        ]
    );

    // Dropped: the menu and the manager are released; the toolkit's connection lives on.
    drop(menu);
    assert_eq!(
        server.wait_for(7)[5..],
        [Seen::Released(sid2), Seen::ManagerReleased]
    );
    queue.roundtrip(&mut Toolkit).unwrap();
    surface.destroy();
    again.destroy();
    queue.roundtrip(&mut Toolkit).unwrap();
}

#[test]
fn no_manager_no_menu() {
    let (server, socket) = Server::start(false);
    let (conn, _compositor, mut queue) = toolkit(socket);
    queue.roundtrip(&mut Toolkit).unwrap();
    let display = conn.backend().display_ptr().cast::<c_void>();
    assert!(unsafe { Appmenu::new(display) }.is_none());
    assert!(unsafe { Appmenu::new(std::ptr::null_mut()) }.is_none());
    queue.roundtrip(&mut Toolkit).unwrap();
    assert!(server.wait_for(0).is_empty());
}

#[test]
fn a_bad_surface_is_refused() {
    let (_server, socket) = Server::start(true);
    let (conn, compositor, mut queue) = toolkit(socket);
    queue.roundtrip(&mut Toolkit).unwrap();
    let display = conn.backend().display_ptr().cast::<c_void>();
    let mut menu = unsafe { Appmenu::new(display) }.unwrap();
    assert!(unsafe { menu.attach(std::ptr::null_mut(), ":1.1", "/MenuBar") }.is_err());
    // A live proxy that isn't a wl_surface.
    assert!(unsafe { menu.attach(ptr(&compositor), ":1.1", "/MenuBar") }.is_err());
}
