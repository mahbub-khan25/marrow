//! Marrow's menus in the Linux desktop's global menu bar (Hyperlume's top bar,
//! KDE Plasma's global menu), on Wayland. Marrow's window has no menu bar of
//! its own; this only lets the desktop show one for it.
//!
//! - [`model`]: the menu (File, Edit, View, Go, Help), its shortcuts, and its
//!   dbusmenu layout.
//! - [`server`]: the menu as `com.canonical.dbusmenu` on the session bus.
//! - [`wayland`]: the KDE appmenu protocol that tells the compositor which
//!   menu belongs to which window.
//!
//! None of it needs GTK; the app glues it to its window. On other systems the
//! crate is empty.

#![cfg(target_os = "linux")]

pub mod model;
pub mod server;
pub mod wayland;

pub use model::{Action, Menu};
/// The D-Bus crate the server is built on, for the app's own calls.
pub use zbus;

pub use server::{
    registrar_present, wait_for_registrar, MenuServer, SharedMenu, OBJECT_PATH, REGISTRAR,
};
