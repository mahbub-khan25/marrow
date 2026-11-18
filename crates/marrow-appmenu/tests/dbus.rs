//! The menu served on a private session bus (its own `dbus-daemon`), read the
//! way a desktop reads it. Skipped where `dbus-daemon` isn't installed.

#![cfg(target_os = "linux")]

use std::collections::HashMap;
use std::io::{BufRead, BufReader};
use std::process::{Child, Command, Stdio};
use std::sync::{mpsc, Arc, Mutex};
use std::time::Duration;

use futures_lite::StreamExt;
use marrow_appmenu::model::plain;
use marrow_appmenu::{
    registrar_present, wait_for_registrar, Action, Menu, MenuServer, OBJECT_PATH, REGISTRAR,
};
use zbus::connection::Builder;
use zbus::zvariant::{OwnedValue, Structure, Value};
use zbus::Connection;

/// A `dbus-daemon` of the test's own, stopped when dropped.
struct Bus {
    daemon: Child,
    address: String,
}

impl Bus {
    fn start() -> Option<Bus> {
        let mut daemon = Command::new("dbus-daemon")
            .args(["--session", "--nofork", "--nopidfile", "--print-address=1"])
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .ok()?;
        let mut line = String::new();
        BufReader::new(daemon.stdout.take()?)
            .read_line(&mut line)
            .ok()?;
        let address = line.trim().to_string();
        if address.is_empty() {
            let _ = daemon.kill();
            return None;
        }
        Some(Bus { daemon, address })
    }

    async fn connect(&self) -> Connection {
        Builder::address(self.address.as_str())
            .unwrap()
            .build()
            .await
            .unwrap()
    }
}

impl Drop for Bus {
    fn drop(&mut self) {
        let _ = self.daemon.kill();
        let _ = self.daemon.wait();
    }
}

type Node = (i32, HashMap<String, OwnedValue>, Vec<OwnedValue>);

fn node(v: &Value<'_>) -> Node {
    let v = match v {
        Value::Value(inner) => inner.as_ref(),
        v => v,
    };
    let s: Structure<'_> = v.try_clone().unwrap().try_into().unwrap();
    s.try_into().unwrap()
}

fn label(props: &HashMap<String, OwnedValue>) -> String {
    String::try_from(props["label"].try_clone().unwrap()).unwrap()
}

/// Every node of a layout, flattened: id → (properties, child ids).
fn flatten(n: Node, out: &mut HashMap<i32, (HashMap<String, OwnedValue>, Vec<i32>)>) {
    let (id, props, children) = n;
    let kids: Vec<Node> = children.iter().map(|c| node(c)).collect();
    out.insert(id, (props, kids.iter().map(|k| k.0).collect()));
    for k in kids {
        flatten(k, out);
    }
}

struct Served {
    _bus: Bus,
    server: MenuServer,
    proxy: zbus::Proxy<'static>,
    clicks: mpsc::Receiver<Action>,
}

async fn serve() -> Option<Served> {
    let Some(bus) = Bus::start() else {
        eprintln!("no dbus-daemon here: skipped");
        return None;
    };
    let app = bus.connect().await;
    let (tx, clicks) = mpsc::channel();
    let tx = Mutex::new(tx);
    let menu = Arc::new(Mutex::new(Menu::marrow()));
    let server = MenuServer::serve(
        &app,
        menu,
        Arc::new(move |a| tx.lock().unwrap().send(a).unwrap()),
    )
    .await
    .unwrap();
    let desktop = bus.connect().await;
    let name = server.bus_name().unwrap();
    assert!(
        name.starts_with(':'),
        "served on the unique name, not {name}"
    );
    let proxy = zbus::Proxy::new_owned(desktop, name, OBJECT_PATH, "com.canonical.dbusmenu")
        .await
        .unwrap();
    Some(Served {
        _bus: bus,
        server,
        proxy,
        clicks,
    })
}

#[tokio::test]
async fn get_layout_returns_the_whole_menu() {
    let Some(s) = serve().await else { return };
    let reply = s
        .proxy
        .call_method("GetLayout", &(0i32, -1i32, Vec::<String>::new()))
        .await
        .unwrap();
    assert_eq!(
        reply.body().signature().to_string_no_parens(),
        "u(ia{sv}av)"
    );
    let (revision, root): (u32, Node) = reply.body().deserialize().unwrap();
    assert!(revision >= 1);
    assert_eq!(root.0, 0);
    let mut all = HashMap::new();
    flatten(root, &mut all);
    let menu = Menu::marrow();
    assert_eq!(all.len(), menu.nodes().len(), "every node is in the reply");

    let (_, top) = &all[&0];
    let names: Vec<String> = top.iter().map(|id| plain(&label(&all[id].0))).collect();
    assert_eq!(names, ["File", "Edit", "View", "Go", "Help"]);
    let file = &all[&top[0]].0;
    assert_eq!(
        String::try_from(file["children-display"].try_clone().unwrap()).unwrap(),
        "submenu"
    );

    let new_note = menu.node_of(Action::NewNote).unwrap().id;
    let (props, _) = &all[&new_note];
    assert_eq!(label(props), "_New note");
    let keys = Vec::<Vec<String>>::try_from(props["shortcut"].try_clone().unwrap()).unwrap();
    assert_eq!(keys, [["Control", "N"]]);

    let sidebar = menu.node_of(Action::Sidebar).unwrap().id;
    let (props, _) = &all[&sidebar];
    assert_eq!(
        String::try_from(props["toggle-type"].try_clone().unwrap()).unwrap(),
        "checkmark"
    );
    assert_eq!(
        i32::try_from(props["toggle-state"].try_clone().unwrap()).unwrap(),
        0
    );

    let separators = all
        .values()
        .filter(|(p, _)| {
            p.get("type")
                .and_then(|t| String::try_from(t.try_clone().unwrap()).ok())
                .as_deref()
                == Some("separator")
        })
        .count();
    assert!(separators > 5);

    // A submenu on its own, one level deep.
    let (_, edit): (u32, Node) = s
        .proxy
        .call("GetLayout", &(top[1], 1i32, vec!["label".to_string()]))
        .await
        .unwrap();
    assert_eq!(edit.0, top[1]);
    assert_eq!(edit.1.keys().collect::<Vec<_>>(), ["label"]);
    let undo = node(&edit.2[0]);
    assert_eq!(label(&undo.1), "_Undo");
    assert!(
        !undo.1.contains_key("shortcut"),
        "only the asked-for properties"
    );

    // No such item.
    let e = s
        .proxy
        .call::<_, _, (u32, Node)>("GetLayout", &(9999i32, -1i32, Vec::<String>::new()))
        .await;
    assert!(e.is_err());
}

#[tokio::test]
async fn clicked_runs_the_action() {
    let Some(s) = serve().await else { return };
    let menu = Menu::marrow();
    for a in [
        Action::NewNote,
        Action::Paste,
        Action::Sidebar,
        Action::About,
    ] {
        let id = menu.node_of(a).unwrap().id;
        s.proxy
            .call::<_, _, ()>("Event", &(id, "clicked", Value::from(0i32), 0u32))
            .await
            .unwrap();
        assert_eq!(s.clicks.recv_timeout(Duration::from_secs(5)).unwrap(), a);
    }
    // Opening a menu or hovering does nothing.
    let file = menu.root().children[0].id;
    s.proxy
        .call::<_, _, ()>("Event", &(file, "opened", Value::from(0i32), 0u32))
        .await
        .unwrap();
    let id = menu.node_of(Action::Save).unwrap().id;
    s.proxy
        .call::<_, _, ()>("Event", &(id, "hovered", Value::from(0i32), 0u32))
        .await
        .unwrap();
    assert!(s.clicks.recv_timeout(Duration::from_millis(200)).is_err());
    // An item that doesn't exist is an error.
    let e = s
        .proxy
        .call::<_, _, ()>("Event", &(4242i32, "clicked", Value::from(0i32), 0u32))
        .await;
    assert!(e.is_err());

    // EventGroup: the click happens, the missing id comes back.
    let copy = menu.node_of(Action::Copy).unwrap().id;
    let events = vec![
        (copy, "clicked", Value::from(0i32), 0u32),
        (4242i32, "clicked", Value::from(0i32), 0u32),
    ];
    let missing: Vec<i32> = s.proxy.call("EventGroup", &(events,)).await.unwrap();
    assert_eq!(missing, [4242]);
    assert_eq!(
        s.clicks.recv_timeout(Duration::from_secs(5)).unwrap(),
        Action::Copy
    );
}

#[tokio::test]
async fn properties_and_about_to_show() {
    let Some(s) = serve().await else { return };
    assert_eq!(s.proxy.get_property::<u32>("Version").await.unwrap(), 3);
    assert_eq!(
        s.proxy.get_property::<String>("Status").await.unwrap(),
        "normal"
    );
    assert_eq!(
        s.proxy
            .get_property::<String>("TextDirection")
            .await
            .unwrap(),
        "ltr"
    );
    assert!(s
        .proxy
        .get_property::<Vec<String>>("IconThemePath")
        .await
        .unwrap()
        .is_empty());

    let menu = Menu::marrow();
    let file = menu.root().children[0].id;
    assert!(!s
        .proxy
        .call::<_, _, bool>("AboutToShow", &(file,))
        .await
        .unwrap());
    let (updates, errors): (Vec<i32>, Vec<i32>) = s
        .proxy
        .call("AboutToShowGroup", &(vec![0i32, file, 777],))
        .await
        .unwrap();
    assert!(updates.is_empty());
    assert_eq!(errors, [777]);

    let copy = menu.node_of(Action::Copy).unwrap().id;
    let v: OwnedValue = s.proxy.call("GetProperty", &(copy, "label")).await.unwrap();
    assert_eq!(String::try_from(v).unwrap(), "_Copy");
    let group: Vec<(i32, HashMap<String, OwnedValue>)> = s
        .proxy
        .call(
            "GetGroupProperties",
            &(vec![copy], vec!["shortcut".to_string()]),
        )
        .await
        .unwrap();
    assert_eq!(group.len(), 1);
    assert_eq!(group[0].0, copy);
    let keys = Vec::<Vec<String>>::try_from(group[0].1["shortcut"].try_clone().unwrap()).unwrap();
    assert_eq!(keys, [["Control", "C"]]);
}

#[tokio::test]
async fn an_update_emits_layout_updated() {
    let Some(s) = serve().await else { return };
    let mut updates = s.proxy.receive_signal("LayoutUpdated").await.unwrap();
    let before = s.server.menu().lock().unwrap().revision();
    s.server
        .update(
            HashMap::from([("menu-file".to_string(), "ফাইল".to_string())]),
            HashMap::from([("sidebar".to_string(), true)]),
            HashMap::new(),
        )
        .await
        .unwrap();
    let msg = tokio::time::timeout(Duration::from_secs(5), updates.next())
        .await
        .unwrap()
        .unwrap();
    let (revision, parent): (u32, i32) = msg.body().deserialize().unwrap();
    assert!(revision > before);
    assert_eq!(parent, 0);

    let (r, root): (u32, Node) = s
        .proxy
        .call("GetLayout", &(0i32, 1i32, Vec::<String>::new()))
        .await
        .unwrap();
    assert_eq!(r, revision);
    assert_eq!(label(&node(&root.2[0]).1), "ফাইল");
    let sidebar = Menu::marrow().node_of(Action::Sidebar).unwrap().id;
    let v: OwnedValue = s
        .proxy
        .call("GetProperty", &(sidebar, "toggle-state"))
        .await
        .unwrap();
    assert_eq!(i32::try_from(v).unwrap(), 1);

    // The same again: no signal.
    s.server
        .update(
            HashMap::from([("menu-file".to_string(), "ফাইল".to_string())]),
            HashMap::from([("sidebar".to_string(), true)]),
            HashMap::new(),
        )
        .await
        .unwrap();
    assert!(
        tokio::time::timeout(Duration::from_millis(300), updates.next())
            .await
            .is_err()
    );
}

#[tokio::test]
async fn the_registrar_is_waited_for() {
    let Some(bus) = Bus::start() else {
        eprintln!("no dbus-daemon here: skipped");
        return;
    };
    let app = bus.connect().await;
    assert!(!registrar_present(&app).await);
    let waiting = tokio::spawn({
        let app = app.clone();
        async move { wait_for_registrar(&app).await }
    });
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert!(!waiting.is_finished());
    let desktop = bus.connect().await;
    desktop.request_name(REGISTRAR).await.unwrap();
    tokio::time::timeout(Duration::from_secs(5), waiting)
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    assert!(registrar_present(&app).await);
    // Already there: at once.
    tokio::time::timeout(Duration::from_secs(5), wait_for_registrar(&app))
        .await
        .unwrap()
        .unwrap();
}

/// GLib (what libdbusmenu-based desktops use) reads the same tree.
#[tokio::test]
async fn glib_reads_the_layout() {
    let Some(s) = serve().await else { return };
    let address = s._bus.address.clone();
    let dest = s.server.bus_name().unwrap();
    let out = tokio::task::spawn_blocking(move || {
        Command::new("gdbus")
            .args([
                "call",
                "--address",
                &address,
                "--dest",
                &dest,
                "--object-path",
                OBJECT_PATH,
            ])
            .args([
                "--method",
                "com.canonical.dbusmenu.GetLayout",
                "0",
                "int32 -1",
                "@as []",
            ])
            .output()
    })
    .await
    .unwrap();
    let Ok(out) = out else {
        eprintln!("no gdbus here: skipped");
        return;
    };
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let text = String::from_utf8(out.stdout).unwrap();
    assert!(text.contains("'children-display': <'submenu'>"), "{text}");
    assert!(
        text.contains("<(1, {'children-display': <'submenu'>, 'label': <'_File'>}")
            || text.contains("<(1, {'label': <'_File'>, 'children-display': <'submenu'>}"),
        "{text}"
    );
    assert!(text.contains("'shortcut': <[['Control', 'N']]>"), "{text}");
    assert!(text.contains("'type': <'separator'>"), "{text}");
    assert!(text.contains("'toggle-type': <'checkmark'>"), "{text}");
}
