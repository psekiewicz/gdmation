//! Cały proces dla zestawu klatek: wspólna paleta → indeksowanie → scalanie bloków.

use std::path::Path;

use anyhow::{Context, Result};
use image::RgbaImage;

use crate::{
    gd::{self, ExportSettings, FrameMesh},
    mesh::{Rect, greedy_mesh},
    palette::{IndexedImage, Palette, build_palette, quantize},
};

pub fn load_png(path: &Path) -> Result<RgbaImage> {
    let img = image::open(path).with_context(|| format!("Nie można wczytać {}", path.display()))?;
    Ok(img.to_rgba8())
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ReduceOptions {
    pub max_colors: usize,
    /// Piksele z alfą poniżej progu są pomijane.
    pub alpha_threshold: u8,
}

impl Default for ReduceOptions {
    fn default() -> Self {
        Self {
            max_colors: 16,
            alpha_threshold: 128,
        }
    }
}

pub struct ProcessedFrame {
    pub indexed: IndexedImage,
    pub rects: Vec<Rect>,
}

pub struct Processed {
    pub palette: Palette,
    pub frames: Vec<ProcessedFrame>,
}

impl Processed {
    /// Liczba obiektów bez scalania (1 piksel = 1 obiekt).
    pub fn objects_before(&self) -> usize {
        self.frames.iter().map(|f| f.indexed.opaque_count()).sum()
    }

    /// Liczba obiektów po scalaniu.
    pub fn objects_after(&self) -> usize {
        self.frames.iter().map(|f| f.rects.len()).sum()
    }

    pub fn build_level(
        &self,
        settings: &ExportSettings,
    ) -> Result<gdlib::cclocallevels::gdlevel::GDLevel> {
        let frames: Vec<FrameMesh> = self
            .frames
            .iter()
            .map(|f| FrameMesh {
                rects: &f.rects,
                height: f.indexed.height,
            })
            .collect();
        gd::build_level(&frames, &self.palette, settings)
    }
}

pub fn process(images: &[RgbaImage], opts: ReduceOptions) -> Processed {
    let palette = build_palette(images, opts.max_colors, opts.alpha_threshold);
    let frames = images
        .iter()
        .map(|img| {
            let indexed = quantize(img, &palette, opts.alpha_threshold);
            let rects = greedy_mesh(&indexed);
            ProcessedFrame { indexed, rects }
        })
        .collect();
    Processed { palette, frames }
}
