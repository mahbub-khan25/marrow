//! Settings: the app's own (in its private folder) and the vault's (in
//! `<vault>/.marrow/app.json`, shared with the desktop app, so they travel
//! with the vault).

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{fail, Marrow, Result};

/// The app's settings on this device.
#[derive(Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub(crate) struct AppSettings {
    pub last_vault: Option<String>,
    /// Vault root → where it syncs.
    pub sync: BTreeMap<String, crate::sync::Target>,
    /// Vault roots whose sync is end-to-end encrypted.
    pub encrypted: BTreeSet<String>,
    /// Vault root → note path → when it was last opened (ms).
    pub seen: BTreeMap<String, BTreeMap<String, i64>>,
    /// Vault root → notes not to suggest again.
    pub skip: BTreeMap<String, Vec<String>>,
}

impl Marrow {
    fn settings_file(&self) -> std::path::PathBuf {
        self.data_dir.join("settings.json")
    }

    pub(crate) fn app_settings(&self) -> AppSettings {
        std::fs::read(self.settings_file()).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default()
    }

    pub(crate) fn save_app_settings(&self, s: &AppSettings) -> Result<()> {
        let bytes = serde_json::to_vec_pretty(s).map_err(|e| fail(e.to_string()))?;
        marrow_core::atomic::write(self.settings_file(), bytes)?;
        Ok(())
    }

    pub(crate) fn update_settings(&self, f: impl FnOnce(&mut AppSettings)) -> Result<()> {
        let mut s = self.app_settings();
        f(&mut s);
        self.save_app_settings(&s)
    }
}

/// Settings that belong to the vault.
#[derive(Debug, Clone, uniffi::Record)]
pub struct VaultSettings {
    /// Where daily notes go, e.g. "Daily".
    pub daily_folder: String,
    /// "year-month" (Daily/2026/10 October/2026-10-24.md) or "flat".
    pub daily_layout: String,
    /// Notes here are offered as templates.
    pub template_folder: String,
    /// Where new notes go: "" for the top of the vault.
    pub new_note_folder: String,
    /// Starred notes, shown first.
    pub starred: Vec<String>,
    /// How new links are written: "shortest", "relative" or "absolute".
    pub link_format: String,
}

fn vault_settings_file(root: &Path) -> std::path::PathBuf {
    root.join(marrow_core::vault::CONFIG_DIR).join("app.json")
}

/// The vault's settings file as JSON, keeping keys the phone apps don't use.
pub(crate) fn read_vault_json(root: &Path) -> Value {
    std::fs::read(vault_settings_file(root))
        .ok()
        .and_then(|b| serde_json::from_slice::<Value>(&b).ok())
        .filter(|v| v.is_object())
        .unwrap_or_else(|| Value::Object(Default::default()))
}

fn str_of(v: &Value, key: &str, default: &str) -> String {
    v.get(key).and_then(Value::as_str).unwrap_or(default).to_string()
}

pub(crate) fn read_vault_settings(root: &Path) -> VaultSettings {
    let v = read_vault_json(root);
    VaultSettings {
        daily_folder: str_of(&v, "dailyFolder", "Daily"),
        daily_layout: str_of(&v, "dailyLayout", "year-month"),
        template_folder: str_of(&v, "templateFolder", "Templates"),
        new_note_folder: str_of(&v, "newNoteFolder", ""),
        starred: v
            .get("starred")
            .and_then(Value::as_array)
            .map(|a| a.iter().filter_map(|x| x.as_str().map(str::to_string)).collect())
            .unwrap_or_default(),
        link_format: str_of(&v, "linkFormat", "shortest"),
    }
}

pub(crate) fn write_vault_settings(root: &Path, s: &VaultSettings) -> Result<()> {
    let mut v = read_vault_json(root);
    let o = v.as_object_mut().ok_or_else(|| fail("settings"))?;
    o.insert("dailyFolder".into(), s.daily_folder.clone().into());
    o.insert("dailyLayout".into(), s.daily_layout.clone().into());
    o.insert("templateFolder".into(), s.template_folder.clone().into());
    o.insert("newNoteFolder".into(), s.new_note_folder.clone().into());
    o.insert("starred".into(), s.starred.clone().into());
    o.insert("linkFormat".into(), s.link_format.clone().into());
    let f = vault_settings_file(root);
    if let Some(d) = f.parent() {
        std::fs::create_dir_all(d)?;
    }
    marrow_core::atomic::write(&f, serde_json::to_vec_pretty(&v).map_err(|e| fail(e.to_string()))?)?;
    Ok(())
}

pub(crate) fn link_format(s: &str) -> marrow_core::LinkFormat {
    serde_json::from_value(Value::String(s.to_string())).unwrap_or(marrow_core::LinkFormat::Shortest)
}

#[uniffi::export]
impl Marrow {
    pub fn vault_settings(&self) -> Result<VaultSettings> {
        Ok(read_vault_settings(&self.root()?))
    }

    pub fn set_vault_settings(&self, settings: VaultSettings) -> Result<()> {
        self.with(|e| {
            write_vault_settings(e.vault().root(), &settings)?;
            e.link_format = link_format(&settings.link_format);
            Ok(())
        })
    }

    /// Stars or unstars a note.
    pub fn set_starred(&self, path: String, starred: bool) -> Result<()> {
        let root = self.root()?;
        let mut s = read_vault_settings(&root);
        s.starred.retain(|p| p != &path);
        if starred {
            s.starred.push(path);
        }
        write_vault_settings(&root, &s)
    }
}
