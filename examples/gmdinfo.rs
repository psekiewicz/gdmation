//! Podgląd pliku .gmd bez GUI:
//! `cargo run --example gmdinfo -- poziom.gmd [kolejny.gmd ...]`
//!
//! Wypisuje format pliku, wersję, liczbę obiektów i użyte ID z proponowanymi kategoriami
//! mapy hitboxów. Służy m.in. do porównania pliku wyeksportowanego z gry z plikiem z GDmation.

use std::path::PathBuf;

use anyhow::Result;
use gdlib::cclocallevels::gdlevel::GDLevel;
use gdmation::hitmap;

fn main() -> Result<()> {
    let paths: Vec<PathBuf> = std::env::args_os().skip(1).map(PathBuf::from).collect();
    if paths.is_empty() {
        anyhow::bail!("podaj co najmniej jeden plik .gmd");
    }
    for path in paths {
        println!("=== {}", path.display());
        let bytes = std::fs::read(&path)?;
        println!("Rozmiar: {} B", bytes.len());
        let head: String = String::from_utf8_lossy(&bytes[..bytes.len().min(160)]).into();
        println!("Początek pliku: {head}");

        let level = match GDLevel::from_gmd(&path) {
            Ok(l) => l,
            Err(e) => {
                println!("gdlib NIE wczytał pliku: {e:?}\n");
                continue;
            }
        };
        println!("Nazwa: {}", level.identity.name);
        println!(
            "Wersja binarna (k50): {:?}, wersja gry (k17): {}",
            level.identity.binary_version.to_binary_version_id(),
            level.identity.game_version
        );

        let raw = level.get_raw_decrypted_data().unwrap_or_default();
        let header_keys: Vec<&str> = raw
            .split(';')
            .next()
            .unwrap_or("")
            .split(',')
            .step_by(2)
            .collect();
        println!("Klucze nagłówka poziomu: {}", header_keys.join(" "));

        let objects = hitmap::parse_objects(&raw);
        println!("Obiektów: {}", objects.len());
        let used = hitmap::used_ids(&objects);
        println!("Rodzajów obiektów: {}", used.len());
        for (id, count) in used.iter().take(40) {
            println!(
                "  {id:>5} ×{count:<6} {:<26} {}",
                hitmap::suggest(*id).label(),
                hitmap::object_name(*id).unwrap_or("(nieznany)")
            );
        }
        if used.len() > 40 {
            println!("  … i {} kolejnych", used.len() - 40);
        }
        println!();
    }
    Ok(())
}
