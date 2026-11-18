//! The menu on the session bus as `com.canonical.dbusmenu`, the interface the
//! desktop's global menu reads (Qt, libdbusmenu and Firefox serve the same).
//!
//! It is served on the connection's unique name (`:1.N`), from Marrow's own
//! process: the desktop accepts a window's menu only from the process that owns
//! the window.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, MutexGuard};

use zbus::fdo;
use zbus::object_server::SignalEmitter;
use zbus::zvariant::{OwnedValue, Value};
use zbus::Connection;

use crate::model::{Action, Layout, Menu};

/// Where the menu is served (the path Qt uses).
pub const OBJECT_PATH: &str = "/MenuBar";

/// The name a global menu takes on the session bus. Without it nobody shows
/// app menus, so Marrow serves none.
pub const REGISTRAR: &str = "com.canonical.AppMenu.Registrar";

/// What happens when an item is clicked.
pub type OnAction = Arc<dyn Fn(Action) + Send + Sync>;

/// The menu, shared between the D-Bus object and whoever updates it.
pub type SharedMenu = Arc<Mutex<Menu>>;

/// The menu, even if a thread panicked while holding it (it is only data).
fn lock(menu: &SharedMenu) -> MutexGuard<'_, Menu> {
    menu.lock().unwrap_or_else(|e| e.into_inner())
}

struct DbusMenu {
    menu: SharedMenu,
    on_action: OnAction,
}

fn no_item(id: i32) -> fdo::Error {
    fdo::Error::InvalidArgs(format!("no menu item {id}"))
}

#[zbus::interface(name = "com.canonical.dbusmenu")]
impl DbusMenu {
    #[zbus(out_args("revision", "layout"))]
    fn get_layout(
        &self,
        parent_id: i32,
        recursion_depth: i32,
        property_names: Vec<String>,
    ) -> fdo::Result<(u32, Layout)> {
        let m = lock(&self.menu);
        let layout = m
            .layout(parent_id, recursion_depth, &property_names)
            .ok_or_else(|| no_item(parent_id))?;
        Ok((m.revision(), layout))
    }

    fn get_group_properties(
        &self,
        ids: Vec<i32>,
        property_names: Vec<String>,
    ) -> Vec<(i32, HashMap<String, Value<'static>>)> {
        let m = lock(&self.menu);
        // No ids: every item.
        let nodes = if ids.is_empty() {
            m.nodes()
        } else {
            ids.iter().filter_map(|id| m.node(*id)).collect()
        };
        nodes
            .into_iter()
            .map(|n| (n.id, m.properties(n, &property_names)))
            .collect()
    }

    fn get_property(&self, id: i32, name: String) -> fdo::Result<Value<'static>> {
        let m = lock(&self.menu);
        m.node(id).ok_or_else(|| no_item(id))?;
        m.property(id, &name).ok_or_else(|| {
            fdo::Error::InvalidArgs(format!("menu item {id} has no property {name}"))
        })
    }

    fn event(
        &self,
        id: i32,
        event_id: String,
        _data: OwnedValue,
        _timestamp: u32,
    ) -> fdo::Result<()> {
        let action = {
            let m = lock(&self.menu);
            m.node(id).ok_or_else(|| no_item(id))?;
            m.event(id, &event_id)
        };
        if let Some(a) = action {
            log::debug!("global menu: {}", a.name());
            (self.on_action)(a);
        }
        Ok(())
    }

    /// Returns the ids that don't exist.
    fn event_group(&self, events: Vec<(i32, String, OwnedValue, u32)>) -> fdo::Result<Vec<i32>> {
        let mut missing = Vec::new();
        let mut actions = Vec::new();
        {
            let m = lock(&self.menu);
            for (id, event, _, _) in &events {
                if m.node(*id).is_none() {
                    missing.push(*id);
                } else if let Some(a) = m.event(*id, event) {
                    actions.push(a);
                }
            }
        }
        if !events.is_empty() && missing.len() == events.len() {
            return Err(fdo::Error::InvalidArgs(
                "none of these menu items exist".into(),
            ));
        }
        for a in actions {
            (self.on_action)(a);
        }
        Ok(missing)
    }

    /// Nothing is filled in late: the layout is always complete.
    fn about_to_show(&self, id: i32) -> fdo::Result<bool> {
        lock(&self.menu).node(id).ok_or_else(|| no_item(id))?;
        Ok(false)
    }

    #[zbus(out_args("updates_needed", "id_errors"))]
    fn about_to_show_group(&self, ids: Vec<i32>) -> (Vec<i32>, Vec<i32>) {
        let m = lock(&self.menu);
        let errors = ids.into_iter().filter(|id| m.node(*id).is_none()).collect();
        (Vec::new(), errors)
    }

    #[zbus(property)]
    fn version(&self) -> u32 {
        3
    }

    #[zbus(property)]
    fn status(&self) -> String {
        "normal".into()
    }

    #[zbus(property)]
    fn text_direction(&self) -> String {
        "ltr".into()
    }

    #[zbus(property)]
    fn icon_theme_path(&self) -> Vec<String> {
        Vec::new()
    }

    #[zbus(signal)]
    async fn layout_updated(
        emitter: &SignalEmitter<'_>,
        revision: u32,
        parent: i32,
    ) -> zbus::Result<()>;

    #[zbus(signal)]
    async fn item_activation_requested(
        emitter: &SignalEmitter<'_>,
        id: i32,
        timestamp: u32,
    ) -> zbus::Result<()>;
}

/// The menu being served on a connection.
pub struct MenuServer {
    conn: Connection,
    menu: SharedMenu,
}

impl MenuServer {
    /// Serves `menu` at [`OBJECT_PATH`] on `conn`; `on_action` runs (on zbus's
    /// thread) for each click.
    pub async fn serve(
        conn: &Connection,
        menu: SharedMenu,
        on_action: OnAction,
    ) -> zbus::Result<MenuServer> {
        let iface = DbusMenu {
            menu: menu.clone(),
            on_action,
        };
        conn.object_server().at(OBJECT_PATH, iface).await?;
        Ok(MenuServer {
            conn: conn.clone(),
            menu,
        })
    }

    /// The connection's unique name (`:1.N`), what the window points the
    /// desktop at.
    pub fn bus_name(&self) -> Option<String> {
        self.conn.unique_name().map(|n| n.to_string())
    }

    pub fn menu(&self) -> &SharedMenu {
        &self.menu
    }

    /// Tells the desktop the menu changed (after [`Menu::update`] said so).
    pub async fn changed(&self) -> zbus::Result<()> {
        let revision = lock(&self.menu).revision();
        let iface = self
            .conn
            .object_server()
            .interface::<_, DbusMenu>(OBJECT_PATH)
            .await?;
        DbusMenu::layout_updated(iface.signal_emitter(), revision, 0).await
    }

    /// Updates the menu (see [`Menu::update`]) and tells the desktop if
    /// anything changed.
    pub async fn update(
        &self,
        labels: HashMap<String, String>,
        checked: HashMap<String, bool>,
        enabled: HashMap<String, bool>,
    ) -> zbus::Result<()> {
        let changed = lock(&self.menu).update(labels, checked, enabled);
        if changed {
            self.changed().await?;
        }
        Ok(())
    }
}

/// Whether a global menu is listening (someone owns [`REGISTRAR`]).
pub async fn registrar_present(conn: &Connection) -> bool {
    let Ok(dbus) = fdo::DBusProxy::new(conn).await else {
        return false;
    };
    match REGISTRAR.try_into() {
        Ok(name) => dbus.name_has_owner(name).await.unwrap_or(false),
        Err(_) => false,
    }
}

/// Waits until a global menu is listening: at once if it is, else when
/// [`REGISTRAR`] gets an owner (a desktop that turns its global menu on later).
pub async fn wait_for_registrar(conn: &Connection) -> zbus::Result<()> {
    use futures_lite::StreamExt;
    let dbus = fdo::DBusProxy::new(conn).await?;
    // Listen first, then look, so an owner arriving in between isn't missed.
    let mut changes = dbus
        .receive_name_owner_changed_with_args(&[(0, REGISTRAR)])
        .await?;
    if registrar_present(conn).await {
        return Ok(());
    }
    while let Some(change) = changes.next().await {
        if change.args().is_ok_and(|a| a.new_owner().is_some()) {
            return Ok(());
        }
    }
    Err(zbus::Error::Failure("the session bus went away".into()))
}
