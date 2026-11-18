//! Marrow's menu: what it holds, in which order, with which keys, and how it is
//! written out in dbusmenu's shape.
//!
//! The labels here are English, with `_` before the mnemonic letter. The window
//! translates them (the menu's Bangla is all in `ui/src/i18n-bn-menu.ts`) and
//! sends the result back with [`Menu::update`], along with which toggles are on
//! and which items can be used just now.

use std::collections::HashMap;

use zbus::zvariant::{Structure, Value};

/// Something a menu item does. Its [`name`](Action::name) is what the window is
/// told, and what it dispatches on.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Action {
    NewNote,
    NewCanvas,
    NewFolder,
    OpenVault,
    CreateVault,
    Import,
    ExportPdf,
    ExportHtml,
    Print,
    Save,
    CloseTab,
    Quit,
    Undo,
    Redo,
    Cut,
    Copy,
    Paste,
    SelectAll,
    Find,
    SearchAll,
    ReplaceAll,
    Settings,
    Sidebar,
    Inspector,
    Split,
    ReadingView,
    FocusMode,
    IconBar,
    StatusBar,
    DarkMode,
    TextBigger,
    TextSmaller,
    TextDefault,
    Graph,
    Canvas,
    Home,
    Today,
    Palette,
    OpenInTab,
    NextTab,
    PrevTab,
    Calendar,
    Tasks,
    Ask,
    Help,
    Shortcuts,
    About,
}

impl Action {
    /// Every action, each once.
    pub const ALL: &'static [Action] = &[
        Action::NewNote,
        Action::NewCanvas,
        Action::NewFolder,
        Action::OpenVault,
        Action::CreateVault,
        Action::Import,
        Action::ExportPdf,
        Action::ExportHtml,
        Action::Print,
        Action::Save,
        Action::CloseTab,
        Action::Quit,
        Action::Undo,
        Action::Redo,
        Action::Cut,
        Action::Copy,
        Action::Paste,
        Action::SelectAll,
        Action::Find,
        Action::SearchAll,
        Action::ReplaceAll,
        Action::Settings,
        Action::Sidebar,
        Action::Inspector,
        Action::Split,
        Action::ReadingView,
        Action::FocusMode,
        Action::IconBar,
        Action::StatusBar,
        Action::DarkMode,
        Action::TextBigger,
        Action::TextSmaller,
        Action::TextDefault,
        Action::Graph,
        Action::Canvas,
        Action::Home,
        Action::Today,
        Action::Palette,
        Action::OpenInTab,
        Action::NextTab,
        Action::PrevTab,
        Action::Calendar,
        Action::Tasks,
        Action::Ask,
        Action::Help,
        Action::Shortcuts,
        Action::About,
    ];

    /// The name the window knows the action by (`ui/src/appmenu.ts`).
    pub fn name(self) -> &'static str {
        match self {
            Action::NewNote => "new-note",
            Action::NewCanvas => "new-canvas",
            Action::NewFolder => "new-folder",
            Action::OpenVault => "open-vault",
            Action::CreateVault => "create-vault",
            Action::Import => "import",
            Action::ExportPdf => "export-pdf",
            Action::ExportHtml => "export-html",
            Action::Print => "print",
            Action::Save => "save",
            Action::CloseTab => "close-tab",
            Action::Quit => "quit",
            Action::Undo => "undo",
            Action::Redo => "redo",
            Action::Cut => "cut",
            Action::Copy => "copy",
            Action::Paste => "paste",
            Action::SelectAll => "select-all",
            Action::Find => "find",
            Action::SearchAll => "search-all",
            Action::ReplaceAll => "replace-all",
            Action::Settings => "settings",
            Action::Sidebar => "sidebar",
            Action::Inspector => "inspector",
            Action::Split => "split",
            Action::ReadingView => "reading-view",
            Action::FocusMode => "focus-mode",
            Action::IconBar => "icon-bar",
            Action::StatusBar => "status-bar",
            Action::DarkMode => "dark-mode",
            Action::TextBigger => "text-bigger",
            Action::TextSmaller => "text-smaller",
            Action::TextDefault => "text-default",
            Action::Graph => "graph",
            Action::Canvas => "canvas",
            Action::Home => "home",
            Action::Today => "today",
            Action::Palette => "palette",
            Action::OpenInTab => "open-in-tab",
            Action::NextTab => "next-tab",
            Action::PrevTab => "prev-tab",
            Action::Calendar => "calendar",
            Action::Tasks => "tasks",
            Action::Ask => "ask",
            Action::Help => "help",
            Action::Shortcuts => "shortcuts",
            Action::About => "about",
        }
    }

    pub fn from_name(name: &str) -> Option<Action> {
        Action::ALL.iter().copied().find(|a| a.name() == name)
    }

    /// Cut, Copy and Paste are done by the web view itself (WebKit's own editing
    /// commands), so they act on whatever has the focus in the window, with the
    /// clipboard access a keypress would have. The name is WebKit's.
    pub fn webview_command(self) -> Option<&'static str> {
        match self {
            Action::Cut => Some("Cut"),
            Action::Copy => Some("Copy"),
            Action::Paste => Some("Paste"),
            _ => None,
        }
    }
}

/// One entry of the menu as it is written down in [`marrow_menu`].
enum Entry {
    Menu(&'static str, &'static str, Vec<Entry>),
    Item(Action, &'static str, &'static [&'static str], bool),
    Separator,
}

/// A plain item: action, label, keys.
fn item(action: Action, label: &'static str, keys: &'static [&'static str]) -> Entry {
    Entry::Item(action, label, keys, false)
}

/// An item with a check mark.
fn check(action: Action, label: &'static str, keys: &'static [&'static str]) -> Entry {
    Entry::Item(action, label, keys, true)
}

/// Marrow's menu. The keys are the shortcuts Marrow already has (`bindKeys` in
/// `ui/src/main.ts` and the editor's keymap); the menu only shows them. Key
/// names follow Qt's dbusmenu exporter: modifiers `Control`, `Shift`, `Alt`,
/// then the key as printed (`N`, `,`, `[`, `Tab`, `F11`).
fn marrow_menu() -> Vec<Entry> {
    use Action::*;
    use Entry::{Menu, Separator};
    const CTRL_SHIFT_BRACKET_L: &[&str] = &["Control", "Shift", "["];
    const CTRL_SHIFT_BRACKET_R: &[&str] = &["Control", "Shift", "]"];
    vec![
        Menu(
            "menu-file",
            "_File",
            vec![
                item(NewNote, "_New note", &["Control", "N"]),
                item(NewCanvas, "New c_anvas", &[]),
                item(NewFolder, "New _folder", &[]),
                Separator,
                item(OpenVault, "_Open another vault…", &[]),
                item(CreateVault, "C_reate a new vault…", &[]),
                Separator,
                item(Import, "_Import from Notion, Evernote, OneNote…", &[]),
                item(ExportPdf, "Export to _PDF…", &[]),
                item(ExportHtml, "Export to _HTML…", &[]),
                item(Print, "Prin_t…", &[]),
                Separator,
                item(Save, "_Save", &["Control", "S"]),
                item(CloseTab, "_Close tab", &["Control", "W"]),
                Separator,
                item(Quit, "_Quit Marrow", &["Control", "Q"]),
            ],
        ),
        Menu(
            "menu-edit",
            "_Edit",
            vec![
                item(Undo, "_Undo", &["Control", "Z"]),
                item(Redo, "_Redo", &["Control", "Shift", "Z"]),
                Separator,
                item(Cut, "Cu_t", &["Control", "X"]),
                item(Copy, "_Copy", &["Control", "C"]),
                item(Paste, "_Paste", &["Control", "V"]),
                item(SelectAll, "Select _all", &["Control", "A"]),
                Separator,
                item(Find, "_Find in this note", &["Control", "F"]),
                item(SearchAll, "_Search all notes", &["Control", "Shift", "F"]),
                item(ReplaceAll, "Find and r_eplace in all notes", &[]),
                Separator,
                item(Settings, "Setti_ngs", &["Control", ","]),
            ],
        ),
        Menu(
            "menu-view",
            "_View",
            vec![
                check(Sidebar, "_Sidebar", CTRL_SHIFT_BRACKET_L),
                check(Inspector, "_Inspector", CTRL_SHIFT_BRACKET_R),
                check(Split, "Split _pane", &["Control", "\\"]),
                check(ReadingView, "_Reading view", &["Control", "E"]),
                check(FocusMode, "_Focus mode", &["F11"]),
                Separator,
                check(IconBar, "I_con bar", &[]),
                check(StatusBar, "Status _bar", &[]),
                check(DarkMode, "_Dark mode", &[]),
                Separator,
                item(TextBigger, "Bigg_er text", &["Control", "="]),
                item(TextSmaller, "S_maller text", &["Control", "-"]),
                item(TextDefault, "_Normal text size", &["Control", "0"]),
                Separator,
                item(Graph, "_Graph view", &["Control", "G"]),
                item(Canvas, "C_anvas", &[]),
            ],
        ),
        Menu(
            "menu-go",
            "_Go",
            vec![
                item(Home, "_Home", &[]),
                item(Today, "_Today's note", &["Control", "D"]),
                item(Palette, "_Search and commands…", &["Control", "K"]),
                item(OpenInTab, "_Open in a new tab…", &["Control", "T"]),
                Separator,
                item(NextTab, "_Next tab", &["Control", "Tab"]),
                item(PrevTab, "_Previous tab", &["Control", "Shift", "Tab"]),
                Separator,
                item(Calendar, "_Calendar", &[]),
                item(Tasks, "T_asks", &[]),
                item(Ask, "As_k your notes", &["Control", "Shift", "J"]),
            ],
        ),
        Menu(
            "menu-help",
            "_Help",
            vec![
                item(Help, "_How Marrow works", &[]),
                item(Shortcuts, "_Keyboard shortcuts", &[]),
                Separator,
                item(About, "_About Marrow", &[]),
            ],
        ),
    ]
}

/// What a node is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Kind {
    /// The invisible root (id 0) whose children are the menus in the bar.
    Root,
    /// A menu or submenu: its stable name (`menu-file`…) and English label.
    Submenu {
        key: &'static str,
        label: &'static str,
    },
    /// An item that does something.
    Item {
        action: Action,
        label: &'static str,
        keys: &'static [&'static str],
        toggle: bool,
    },
    Separator,
}

/// One node of the menu tree; ids are dbusmenu's, numbered in order from 0.
#[derive(Debug, Clone)]
pub struct Node {
    pub id: i32,
    pub kind: Kind,
    pub children: Vec<Node>,
}

impl Node {
    /// The name the window uses for this node's label, enabled and checked
    /// state: the action's name or the submenu's key.
    pub fn key(&self) -> Option<&'static str> {
        match &self.kind {
            Kind::Submenu { key, .. } => Some(key),
            Kind::Item { action, .. } => Some(action.name()),
            Kind::Root | Kind::Separator => None,
        }
    }

    fn english(&self) -> Option<&'static str> {
        match &self.kind {
            Kind::Submenu { label, .. } | Kind::Item { label, .. } => Some(label),
            Kind::Root | Kind::Separator => None,
        }
    }

    fn find(&self, id: i32) -> Option<&Node> {
        if self.id == id {
            return Some(self);
        }
        self.children.iter().find_map(|c| c.find(id))
    }

    fn walk<'a>(&'a self, out: &mut Vec<&'a Node>) {
        out.push(self);
        for c in &self.children {
            c.walk(out);
        }
    }
}

/// A label without its mnemonic marks: `"_New note"` → `"New note"`, `"a__b"` → `"a_b"`.
pub fn plain(label: &str) -> String {
    let mut out = String::with_capacity(label.len());
    let mut chars = label.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '_' {
            if chars.peek() == Some(&'_') {
                out.push('_');
                chars.next();
            }
            continue;
        }
        out.push(c);
    }
    out
}

/// A label shown as it is, with no mnemonic: each `_` doubled.
fn escape(label: &str) -> String {
    label.replace('_', "__")
}

/// The layout node of `GetLayout`: `(ia{sv}av)`.
pub type Layout = (i32, HashMap<String, Value<'static>>, Vec<Value<'static>>);

/// The menu and what the window last said about it.
#[derive(Debug, Clone)]
pub struct Menu {
    root: Node,
    revision: u32,
    labels: HashMap<String, String>,
    checked: HashMap<String, bool>,
    enabled: HashMap<String, bool>,
}

impl Default for Menu {
    fn default() -> Self {
        Menu::marrow()
    }
}

impl Menu {
    /// Marrow's menu with English labels, every item usable, no toggle on.
    pub fn marrow() -> Menu {
        fn build(entries: Vec<Entry>, next: &mut i32) -> Vec<Node> {
            entries
                .into_iter()
                .map(|e| {
                    let id = *next;
                    *next += 1;
                    match e {
                        Entry::Menu(key, label, children) => Node {
                            id,
                            kind: Kind::Submenu { key, label },
                            children: build(children, next),
                        },
                        Entry::Item(action, label, keys, toggle) => Node {
                            id,
                            kind: Kind::Item {
                                action,
                                label,
                                keys,
                                toggle,
                            },
                            children: Vec::new(),
                        },
                        Entry::Separator => Node {
                            id,
                            kind: Kind::Separator,
                            children: Vec::new(),
                        },
                    }
                })
                .collect()
        }
        let mut next = 1;
        let children = build(marrow_menu(), &mut next);
        Menu {
            root: Node {
                id: 0,
                kind: Kind::Root,
                children,
            },
            revision: 1,
            labels: HashMap::new(),
            checked: HashMap::new(),
            enabled: HashMap::new(),
        }
    }

    pub fn root(&self) -> &Node {
        &self.root
    }

    /// Goes up by one each time the menu changes.
    pub fn revision(&self) -> u32 {
        self.revision
    }

    pub fn node(&self, id: i32) -> Option<&Node> {
        self.root.find(id)
    }

    /// Every node, the root first, in menu order.
    pub fn nodes(&self) -> Vec<&Node> {
        let mut out = Vec::new();
        self.root.walk(&mut out);
        out
    }

    /// The node of an action.
    pub fn node_of(&self, action: Action) -> Option<&Node> {
        self.nodes()
            .into_iter()
            .find(|n| matches!(n.kind, Kind::Item { action: a, .. } if a == action))
    }

    /// Each menu and item's name and English label (without mnemonics), for
    /// the window to translate.
    pub fn english_labels(&self) -> Vec<(String, String)> {
        self.nodes()
            .into_iter()
            .filter_map(|n| Some((n.key()?.to_string(), plain(n.english()?))))
            .collect()
    }

    /// What the window says now: labels in its language (none: English),
    /// which toggles are on, which items can be used. True if anything shown
    /// changed (the revision went up).
    pub fn update(
        &mut self,
        labels: HashMap<String, String>,
        checked: HashMap<String, bool>,
        enabled: HashMap<String, bool>,
    ) -> bool {
        // An empty or unchanged translation is no translation.
        let labels: HashMap<String, String> = self
            .nodes()
            .into_iter()
            .filter_map(|n| {
                let key = n.key()?;
                let l = labels.get(key)?.trim();
                (!l.is_empty() && l != plain(n.english()?))
                    .then(|| (key.to_string(), l.to_string()))
            })
            .collect();
        let checked: HashMap<String, bool> = checked.into_iter().filter(|(_, v)| *v).collect();
        let enabled: HashMap<String, bool> = enabled.into_iter().filter(|(_, v)| !*v).collect();
        if labels == self.labels && checked == self.checked && enabled == self.enabled {
            return false;
        }
        self.labels = labels;
        self.checked = checked;
        self.enabled = enabled;
        self.revision = self.revision.wrapping_add(1).max(1);
        true
    }

    /// The label as dbusmenu shows it: the translation as it is, or the
    /// English with its mnemonic.
    fn label(&self, n: &Node) -> Option<String> {
        let key = n.key()?;
        Some(match self.labels.get(key) {
            Some(l) => escape(l),
            None => n.english()?.to_string(),
        })
    }

    /// A node's properties. Only those that differ from dbusmenu's defaults
    /// (`enabled` true, `visible` true, `type` "standard") are sent, as the
    /// spec asks; `names` limits them (empty: all).
    pub fn properties(&self, n: &Node, names: &[String]) -> HashMap<String, Value<'static>> {
        let mut p: HashMap<String, Value<'static>> = HashMap::new();
        match &n.kind {
            Kind::Root => {
                p.insert("children-display".into(), Value::from("submenu"));
            }
            Kind::Separator => {
                p.insert("type".into(), Value::from("separator"));
            }
            Kind::Submenu { key, .. } => {
                p.insert("children-display".into(), Value::from("submenu"));
                if let Some(l) = self.label(n) {
                    p.insert("label".into(), Value::from(l));
                }
                if self.enabled.contains_key(*key) {
                    p.insert("enabled".into(), Value::from(false));
                }
            }
            Kind::Item {
                action,
                keys,
                toggle,
                ..
            } => {
                if let Some(l) = self.label(n) {
                    p.insert("label".into(), Value::from(l));
                }
                if self.enabled.contains_key(action.name()) {
                    p.insert("enabled".into(), Value::from(false));
                }
                if !keys.is_empty() {
                    let chord: Vec<String> = keys.iter().map(|k| k.to_string()).collect();
                    p.insert("shortcut".into(), Value::from(vec![chord]));
                }
                if *toggle {
                    p.insert("toggle-type".into(), Value::from("checkmark"));
                    let on = self.checked.contains_key(action.name());
                    p.insert("toggle-state".into(), Value::from(i32::from(on)));
                }
            }
        }
        if !names.is_empty() {
            p.retain(|k, _| names.iter().any(|n| n == k));
        }
        p
    }

    /// One property; `None` for a node or property that doesn't exist.
    /// Properties left at their default are answered with the default.
    pub fn property(&self, id: i32, name: &str) -> Option<Value<'static>> {
        let n = self.node(id)?;
        if let Some(v) = self.properties(n, &[]).remove(name) {
            return Some(v);
        }
        match name {
            "enabled" | "visible" => Some(Value::from(true)),
            "type" => Some(Value::from("standard")),
            "label" if n.key().is_some() => Some(Value::from(self.label(n).unwrap_or_default())),
            _ => None,
        }
    }

    /// `GetLayout(parent, depth, names)`: the subtree under `parent`, `depth`
    /// levels down (-1: all of it).
    pub fn layout(&self, parent: i32, depth: i32, names: &[String]) -> Option<Layout> {
        Some(self.layout_of(self.node(parent)?, depth, names))
    }

    fn layout_of(&self, n: &Node, depth: i32, names: &[String]) -> Layout {
        let children = if depth == 0 {
            Vec::new()
        } else {
            n.children
                .iter()
                .map(|c| Value::Structure(Structure::from(self.layout_of(c, depth - 1, names))))
                .collect()
        };
        (n.id, self.properties(n, names), children)
    }

    /// What `Event(id, event, …)` asks for: a click on a usable item gives
    /// its action; anything else (opened, closed, hovered, a menu, a
    /// separator, an unusable item) nothing.
    pub fn event(&self, id: i32, event: &str) -> Option<Action> {
        if event != "clicked" {
            return None;
        }
        match self.node(id)?.kind {
            Kind::Item { action, .. } if !self.enabled.contains_key(action.name()) => Some(action),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;
    use zbus::zvariant::{Array, Dict};

    fn menu(key: &str, m: &Menu) -> i32 {
        m.nodes()
            .into_iter()
            .find(|n| n.key() == Some(key))
            .unwrap()
            .id
    }

    fn s(v: &Value<'_>) -> String {
        match v {
            Value::Str(s) => s.to_string(),
            other => panic!("not a string: {other:?}"),
        }
    }

    #[test]
    fn names_round_trip_and_are_unique() {
        let mut seen = HashSet::new();
        for a in Action::ALL {
            assert!(seen.insert(a.name()), "{} twice", a.name());
            assert_eq!(Action::from_name(a.name()), Some(*a));
        }
        assert_eq!(Action::from_name("nope"), None);
    }

    #[test]
    fn every_action_is_in_the_menu_once() {
        let m = Menu::marrow();
        for a in Action::ALL {
            let n = m
                .nodes()
                .into_iter()
                .filter(|n| matches!(n.kind, Kind::Item { action, .. } if action == *a))
                .count();
            assert_eq!(n, 1, "{a:?}");
        }
    }

    #[test]
    fn ids_are_unique_and_in_order() {
        let m = Menu::marrow();
        let ids: Vec<i32> = m.nodes().iter().map(|n| n.id).collect();
        assert_eq!(ids, (0..ids.len() as i32).collect::<Vec<_>>());
        assert_eq!(m.root().kind, Kind::Root);
    }

    #[test]
    fn the_bar_has_file_edit_view_go_help() {
        let m = Menu::marrow();
        let top: Vec<String> = m
            .root()
            .children
            .iter()
            .map(|n| plain(n.english().unwrap()))
            .collect();
        assert_eq!(top, ["File", "Edit", "View", "Go", "Help"]);
    }

    #[test]
    fn mnemonics_are_one_per_label_and_unique_per_menu() {
        let m = Menu::marrow();
        for menu in std::iter::once(m.root()).chain(m.root().children.iter()) {
            let mut letters = HashSet::new();
            for n in &menu.children {
                let Some(l) = n.english() else { continue };
                assert!(!l.contains("__"), "{l}");
                assert_eq!(l.matches('_').count(), 1, "{l}");
                let c = l[l.find('_').unwrap() + 1..].chars().next().unwrap();
                assert!(
                    letters.insert(c.to_ascii_lowercase()),
                    "{l}: '{c}' used twice in its menu"
                );
            }
        }
    }

    #[test]
    fn plain_and_escape() {
        assert_eq!(plain("_New note"), "New note");
        assert_eq!(plain("a__b_c"), "a_bc");
        assert_eq!(escape("a_b"), "a__b");
        assert_eq!(plain(&escape("snake_case")), "snake_case");
    }

    #[test]
    fn english_labels_cover_every_named_node() {
        let m = Menu::marrow();
        let l: HashMap<String, String> = m.english_labels().into_iter().collect();
        assert_eq!(l["menu-file"], "File");
        assert_eq!(l["new-note"], "New note");
        assert_eq!(l["open-vault"], "Open another vault…");
        assert_eq!(l["today"], "Today's note");
        let named = m.nodes().iter().filter(|n| n.key().is_some()).count();
        assert_eq!(l.len(), named);
    }

    #[test]
    fn item_properties() {
        let m = Menu::marrow();
        let n = m.node_of(Action::NewNote).unwrap();
        let p = m.properties(n, &[]);
        assert_eq!(s(&p["label"]), "_New note");
        assert!(!p.contains_key("enabled"), "enabled is the default");
        assert!(!p.contains_key("toggle-type"));
        let want = Value::from(vec![vec!["Control".to_string(), "N".to_string()]]);
        assert_eq!(p["shortcut"], want);
        assert_eq!(p["shortcut"].value_signature().to_string(), "aas");

        let p = m.properties(m.node_of(Action::Sidebar).unwrap(), &[]);
        assert_eq!(s(&p["toggle-type"]), "checkmark");
        assert_eq!(p["toggle-state"], Value::from(0i32));
        assert_eq!(
            p["shortcut"],
            Value::from(vec![vec![
                "Control".to_string(),
                "Shift".into(),
                "[".into()
            ]])
        );

        // No shortcut: no property.
        let p = m.properties(m.node_of(Action::NewCanvas).unwrap(), &[]);
        assert!(!p.contains_key("shortcut"));
    }

    #[test]
    fn shortcuts_shown_match_marrows_keys() {
        let m = Menu::marrow();
        let keys = |a: Action| match m.node_of(a).unwrap().kind {
            Kind::Item { keys, .. } => keys.join("+"),
            _ => unreachable!(),
        };
        assert_eq!(keys(Action::Palette), "Control+K");
        assert_eq!(keys(Action::Settings), "Control+,");
        assert_eq!(keys(Action::Split), "Control+\\");
        assert_eq!(keys(Action::FocusMode), "F11");
        assert_eq!(keys(Action::SearchAll), "Control+Shift+F");
        assert_eq!(keys(Action::Ask), "Control+Shift+J");
        assert_eq!(keys(Action::Quit), "Control+Q");
        assert_eq!(keys(Action::TextBigger), "Control+=");
        assert_eq!(keys(Action::TextSmaller), "Control+-");
        assert_eq!(keys(Action::TextDefault), "Control+0");
        assert_eq!(keys(Action::Today), "Control+D");
    }

    #[test]
    fn separator_and_menu_properties() {
        let m = Menu::marrow();
        let sep = m
            .nodes()
            .into_iter()
            .find(|n| n.kind == Kind::Separator)
            .unwrap();
        let p = m.properties(sep, &[]);
        assert_eq!(s(&p["type"]), "separator");
        assert!(!p.contains_key("label"));

        let file = m.node(menu("menu-file", &m)).unwrap();
        let p = m.properties(file, &[]);
        assert_eq!(s(&p["children-display"]), "submenu");
        assert_eq!(s(&p["label"]), "_File");

        let root = m.properties(m.root(), &[]);
        assert_eq!(s(&root["children-display"]), "submenu");
    }

    #[test]
    fn property_names_filter() {
        let m = Menu::marrow();
        let n = m.node_of(Action::Sidebar).unwrap();
        let p = m.properties(n, &["label".into(), "toggle-state".into()]);
        let mut keys: Vec<&String> = p.keys().collect();
        keys.sort();
        assert_eq!(keys, ["label", "toggle-state"]);
    }

    #[test]
    fn single_property_with_defaults() {
        let m = Menu::marrow();
        let id = m.node_of(Action::Copy).unwrap().id;
        assert_eq!(s(&m.property(id, "label").unwrap()), "_Copy");
        assert_eq!(m.property(id, "enabled"), Some(Value::from(true)));
        assert_eq!(m.property(id, "visible"), Some(Value::from(true)));
        assert_eq!(s(&m.property(id, "type").unwrap()), "standard");
        assert_eq!(m.property(id, "icon-name"), None);
        assert_eq!(m.property(9999, "label"), None);
    }

    #[test]
    fn layout_shape_and_signature() {
        let m = Menu::marrow();
        let (id, props, children) = m.layout(0, -1, &[]).unwrap();
        assert_eq!(id, 0);
        assert!(props.contains_key("children-display"));
        assert_eq!(children.len(), 5);
        let Value::Structure(file) = &children[0] else {
            panic!("not a structure")
        };
        assert_eq!(file.signature().to_string(), "(ia{sv}av)");
        let f = file.fields();
        assert_eq!(f[0], Value::from(1i32));
        let Value::Dict(d) = &f[1] else { panic!() };
        let label: Option<String> = d.get(&"label").unwrap();
        assert_eq!(label.as_deref(), Some("_File"));
        let Value::Array(items) = &f[2] else { panic!() };
        assert_eq!(items.len(), m.root().children[0].children.len());
        // The whole reply, as GetLayout sends it.
        let sig = <(u32, Layout) as zbus::zvariant::Type>::SIGNATURE.to_string();
        assert_eq!(sig, "(u(ia{sv}av))");
    }

    #[test]
    fn layout_depth_and_parent() {
        let m = Menu::marrow();
        let (_, _, top) = m.layout(0, 1, &[]).unwrap();
        assert_eq!(top.len(), 5);
        for c in &top {
            let Value::Structure(s) = c else { panic!() };
            let Value::Array(a) = &s.fields()[2] else {
                panic!()
            };
            assert!(a.is_empty(), "depth 1 stops at the menus");
        }
        let (_, _, none) = m.layout(0, 0, &[]).unwrap();
        assert!(none.is_empty());
        let edit = menu("menu-edit", &m);
        let (id, props, items) = m.layout(edit, -1, &[]).unwrap();
        assert_eq!(id, edit);
        assert_eq!(s(&props["label"]), "_Edit");
        assert_eq!(items.len(), m.node(edit).unwrap().children.len());
        assert!(m.layout(12345, -1, &[]).is_none());
    }

    #[test]
    fn layout_children_hold_labels_and_shortcuts() {
        let m = Menu::marrow();
        let edit = menu("menu-edit", &m);
        let (_, _, items) = m.layout(edit, -1, &[]).unwrap();
        let Value::Structure(undo) = &items[0] else {
            panic!()
        };
        let Value::Dict(d) = &undo.fields()[1] else {
            panic!()
        };
        let label: Option<String> = d.get(&"label").unwrap();
        assert_eq!(label.as_deref(), Some("_Undo"));
        let keys: Option<Array> = d.get(&"shortcut").unwrap();
        assert!(keys.is_some());
        let _: &Dict = d;
    }

    #[test]
    fn clicks_give_actions() {
        let m = Menu::marrow();
        for a in Action::ALL {
            let id = m.node_of(*a).unwrap().id;
            assert_eq!(m.event(id, "clicked"), Some(*a));
            assert_eq!(m.event(id, "opened"), None);
            assert_eq!(m.event(id, "hovered"), None);
        }
        assert_eq!(m.event(0, "clicked"), None, "the root");
        assert_eq!(m.event(menu("menu-file", &m), "clicked"), None, "a menu");
        let sep = m
            .nodes()
            .into_iter()
            .find(|n| n.kind == Kind::Separator)
            .unwrap()
            .id;
        assert_eq!(m.event(sep, "clicked"), None, "a separator");
        assert_eq!(m.event(4242, "clicked"), None, "no such item");
    }

    #[test]
    fn updates_translate_check_and_disable() {
        let mut m = Menu::marrow();
        let r0 = m.revision();
        let labels = HashMap::from([
            ("menu-file".to_string(), "ফাইল".to_string()),
            ("new-note".to_string(), "নতুন নোট".to_string()),
            ("copy".to_string(), "Copy".to_string()), // the English: no translation
            ("snake".to_string(), "x".to_string()),   // not in the menu
        ]);
        let checked = HashMap::from([("sidebar".to_string(), true), ("split".to_string(), false)]);
        let enabled = HashMap::from([("save".to_string(), false), ("undo".to_string(), true)]);
        assert!(m.update(labels.clone(), checked.clone(), enabled.clone()));
        assert!(m.revision() > r0);
        // The same again changes nothing.
        let r1 = m.revision();
        assert!(!m.update(labels, checked, enabled));
        assert_eq!(m.revision(), r1);

        let file = m.node(menu("menu-file", &m)).unwrap();
        assert_eq!(s(&m.properties(file, &[])["label"]), "ফাইল");
        let p = m.properties(m.node_of(Action::NewNote).unwrap(), &[]);
        assert_eq!(s(&p["label"]), "নতুন নোট");
        let p = m.properties(m.node_of(Action::Copy).unwrap(), &[]);
        assert_eq!(s(&p["label"]), "_Copy", "English keeps its mnemonic");
        let p = m.properties(m.node_of(Action::Sidebar).unwrap(), &[]);
        assert_eq!(p["toggle-state"], Value::from(1i32));
        let p = m.properties(m.node_of(Action::Split).unwrap(), &[]);
        assert_eq!(p["toggle-state"], Value::from(0i32));
        let save = m.node_of(Action::Save).unwrap();
        assert_eq!(m.properties(save, &[])["enabled"], Value::from(false));
        assert_eq!(
            m.event(save.id, "clicked"),
            None,
            "an unusable item does nothing"
        );
        let undo = m.node_of(Action::Undo).unwrap();
        assert!(!m.properties(undo, &[]).contains_key("enabled"));

        // Back to English.
        assert!(m.update(HashMap::new(), HashMap::new(), HashMap::new()));
        let p = m.properties(m.node_of(Action::NewNote).unwrap(), &[]);
        assert_eq!(s(&p["label"]), "_New note");
    }

    #[test]
    fn a_translation_with_an_underscore_shows_it() {
        let mut m = Menu::marrow();
        m.update(
            HashMap::from([("new-note".to_string(), "new_note".to_string())]),
            HashMap::new(),
            HashMap::new(),
        );
        let p = m.properties(m.node_of(Action::NewNote).unwrap(), &[]);
        assert_eq!(s(&p["label"]), "new__note");
    }

    #[test]
    fn only_cut_copy_paste_go_to_the_webview() {
        let native: Vec<Action> = Action::ALL
            .iter()
            .copied()
            .filter(|a| a.webview_command().is_some())
            .collect();
        assert_eq!(native, [Action::Cut, Action::Copy, Action::Paste]);
    }
}
