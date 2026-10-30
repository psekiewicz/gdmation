//! Zapis poziomu do wybranego folderu gry, bez tworzenia duplikatów.
//!
//! Jedna nazwa poziomu = jeden plik `.gmd`. Ponowne wysłanie tej samej animacji
//! nadpisuje istniejący plik (również gdy różni się tylko wielkością liter).

use std::{
    fs,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, bail};
use gdlib::cclocallevels::gdlevel::GDLevel;

/// Nazwy zarezerwowane przez Windows, których nie można użyć jako nazwy pliku.
const RESERVED: [&str; 22] = [
    "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8",
    "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
];

/// Nazwa pliku `.gmd` dla poziomu: niedozwolone znaki zamienione na `_`.
pub fn level_file_name(level_name: &str) -> String {
    let mut stem: String = level_name
        .chars()
        .map(|c| match c {
            '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*' => '_',
            c if c.is_control() => '_',
            c => c,
        })
        .collect();
    stem = stem.trim().trim_end_matches('.').to_string();
    if stem.is_empty() {
        stem = "GDmation".into();
    }
    if RESERVED.iter().any(|r| r.eq_ignore_ascii_case(&stem)) {
        stem.insert(0, '_');
    }
    format!("{stem}.gmd")
}

/// Ścieżka, pod którą trafi poziom. Jeśli w folderze jest już plik o tej nazwie
/// (bez względu na wielkość liter), zwracana jest jego ścieżka, żeby go nadpisać.
pub fn target_path(folder: &Path, level_name: &str) -> PathBuf {
    let name = level_file_name(level_name);
    let existing = fs::read_dir(folder).ok().and_then(|entries| {
        entries.filter_map(|e| e.ok()).map(|e| e.path()).find(|p| {
            p.is_file()
                && p.file_name()
                    .is_some_and(|n| n.to_string_lossy().eq_ignore_ascii_case(&name))
        })
    });
    existing.unwrap_or_else(|| folder.join(name))
}

#[derive(Debug, Clone, PartialEq)]
pub struct SendOutcome {
    pub path: PathBuf,
    /// `true`, gdy nadpisano wcześniejszą wersję.
    pub replaced: bool,
}

/// Zapisuje poziom do folderu gry. Najpierw do pliku tymczasowego, potem podmiana,
/// żeby przerwany zapis nie zostawił uszkodzonego `.gmd`.
pub fn send_to_folder(level: &GDLevel, folder: &Path) -> Result<SendOutcome> {
    if !folder.is_dir() {
        bail!("Folder gry nie istnieje: {}", folder.display());
    }
    let path = target_path(folder, &level.identity.name);
    let replaced = path.exists();
    let tmp = path.with_extension("gmd.tmp");

    level
        .export_to_gmd(&tmp)
        .map_err(|e| anyhow::anyhow!("{e:?}"))
        .with_context(|| format!("Nie udało się zapisać {}", tmp.display()))?;
    // Na Windows `rename` nadpisuje istniejący plik (MoveFileExW z MOVEFILE_REPLACE_EXISTING).
    if let Err(e) = fs::rename(&tmp, &path) {
        let _ = fs::remove_file(&tmp);
        return Err(e).with_context(|| format!("Nie udało się zapisać {}", path.display()));
    }
    Ok(SendOutcome { path, replaced })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn level(name: &str) -> GDLevel {
        let mut l = GDLevel::default();
        l.identity.name = name.into();
        l
    }

    fn gmd_files(dir: &Path) -> Vec<String> {
        let mut v: Vec<String> = fs::read_dir(dir)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        v.sort();
        v
    }

    #[test]
    fn file_names_are_safe() {
        assert_eq!(level_file_name("krew_chlust"), "krew_chlust.gmd");
        assert_eq!(level_file_name("a/b:c?"), "a_b_c_.gmd");
        assert_eq!(level_file_name("  "), "GDmation.gmd");
        assert_eq!(level_file_name("con"), "_con.gmd");
        assert_eq!(level_file_name("koniec..."), "koniec.gmd");
    }

    #[test]
    fn sending_twice_does_not_duplicate() {
        let dir = tempfile::tempdir().unwrap();
        let first = send_to_folder(&level("krew"), dir.path()).unwrap();
        assert!(!first.replaced);
        let second = send_to_folder(&level("krew"), dir.path()).unwrap();
        assert!(second.replaced);
        assert_eq!(first.path, second.path);
        assert_eq!(gmd_files(dir.path()), vec!["krew.gmd"]);
    }

    #[test]
    fn existing_file_with_other_case_is_reused() {
        let dir = tempfile::tempdir().unwrap();
        send_to_folder(&level("Krew"), dir.path()).unwrap();
        let out = send_to_folder(&level("KREW"), dir.path()).unwrap();
        assert!(out.replaced);
        assert_eq!(gmd_files(dir.path()), vec!["Krew.gmd"]);
        // zawartość to nowa wersja
        assert_eq!(GDLevel::from_gmd(&out.path).unwrap().identity.name, "KREW");
    }

    #[test]
    fn different_levels_get_different_files() {
        let dir = tempfile::tempdir().unwrap();
        send_to_folder(&level("krew"), dir.path()).unwrap();
        send_to_folder(&level("uderzenie"), dir.path()).unwrap();
        assert_eq!(gmd_files(dir.path()), vec!["krew.gmd", "uderzenie.gmd"]);
    }

    #[test]
    fn missing_folder_is_an_error() {
        let dir = tempfile::tempdir().unwrap();
        assert!(send_to_folder(&level("x"), &dir.path().join("brak")).is_err());
    }
}
