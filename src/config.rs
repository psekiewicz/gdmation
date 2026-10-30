//! Ustawienia programu zapamiętywane między uruchomieniami
//! (Windows: `%APPDATA%\GDmation\config.json`).

use std::{fs, path::PathBuf};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct AppConfig {
    /// Folder, do którego „Wyślij do gry” zapisuje pliki `.gmd`.
    pub game_folder: Option<PathBuf>,
}

fn config_path() -> Option<PathBuf> {
    let base = std::env::var_os("APPDATA")
        .or_else(|| std::env::var_os("XDG_CONFIG_HOME"))
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))?;
    Some(base.join("GDmation").join("config.json"))
}

impl AppConfig {
    /// Brak pliku albo błędny plik = ustawienia domyślne.
    pub fn load() -> Self {
        config_path()
            .and_then(|p| fs::read_to_string(p).ok())
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    }

    pub fn save(&self) -> anyhow::Result<()> {
        let path = config_path().ok_or_else(|| anyhow::anyhow!("Brak folderu na ustawienia."))?;
        if let Some(dir) = path.parent() {
            fs::create_dir_all(dir)?;
        }
        fs::write(path, serde_json::to_string_pretty(self)?)?;
        Ok(())
    }
}
