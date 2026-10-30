//! Wersja bez GUI: `cargo run --example png2gmd -- klatka.png wynik.gmd [maks_kolorów]`

use std::path::PathBuf;

use anyhow::{Context, Result};
use gdmation::{
    gd::{self, ExportSettings},
    pipeline::{self, ReduceOptions},
};

fn main() -> Result<()> {
    let mut args = std::env::args_os().skip(1);
    let input = PathBuf::from(args.next().context("podaj plik PNG")?);
    let output = PathBuf::from(args.next().context("podaj plik wynikowy .gmd")?);
    let max_colors = args
        .next()
        .map(|s| s.to_string_lossy().parse())
        .transpose()?
        .unwrap_or(ReduceOptions::default().max_colors);

    let image = pipeline::load_png(&input)?;
    let processed = pipeline::process(
        &[image],
        ReduceOptions {
            max_colors,
            ..Default::default()
        },
    );
    let settings = ExportSettings {
        level_name: input
            .file_stem()
            .unwrap_or_default()
            .to_string_lossy()
            .into(),
        ..Default::default()
    };
    let level = processed.build_level(&settings)?;
    gd::export_gmd(&level, &output)?;

    println!("Kolory: {}", processed.palette.colors.len());
    println!(
        "Obiekty: {} -> {}",
        processed.objects_before(),
        processed.objects_after()
    );
    println!("Zapisano {}", output.display());
    Ok(())
}
