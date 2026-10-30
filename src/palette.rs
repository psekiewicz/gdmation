//! Redukcja kolorów do ograniczonej palety (median cut) i indeksowanie obrazu.
//!
//! Każdy kolor palety odpowiada później jednemu kanałowi koloru w GD.

use std::collections::HashMap;

use image::RgbaImage;
use serde::{Deserialize, Serialize};

pub type Rgb = [u8; 3];

/// Obraz po redukcji palety: `None` = piksel przezroczysty (brak obiektu).
#[derive(Debug, Clone, PartialEq)]
pub struct IndexedImage {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<Option<u8>>,
}

impl IndexedImage {
    pub fn get(&self, x: u32, y: u32) -> Option<u8> {
        self.pixels[(y * self.width + x) as usize]
    }

    /// Liczba nieprzezroczystych pikseli = liczba obiektów bez optymalizacji.
    pub fn opaque_count(&self) -> usize {
        self.pixels.iter().filter(|p| p.is_some()).count()
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Palette {
    pub colors: Vec<Rgb>,
}

/// Maksymalna liczba kolorów: indeks palety musi zmieścić się w `u8`.
pub const MAX_COLORS: usize = 255;

fn is_opaque(a: u8, alpha_threshold: u8) -> bool {
    a >= alpha_threshold
}

fn histogram(images: &[RgbaImage], alpha_threshold: u8) -> HashMap<Rgb, u64> {
    let mut hist = HashMap::new();
    for img in images {
        for p in img.pixels() {
            if is_opaque(p[3], alpha_threshold) {
                *hist.entry([p[0], p[1], p[2]]).or_insert(0) += 1;
            }
        }
    }
    hist
}

/// Buduje wspólną paletę dla wszystkich klatek (wspólne kanały kolorów).
///
/// Jeśli obraz ma nie więcej kolorów niż `max_colors`, kolory są zachowane dokładnie.
/// Kolejność: od najczęstszego, remisy rozstrzyga wartość RGB (wynik deterministyczny).
pub fn build_palette(images: &[RgbaImage], max_colors: usize, alpha_threshold: u8) -> Palette {
    let max_colors = max_colors.clamp(1, MAX_COLORS);
    let hist = histogram(images, alpha_threshold);
    let mut entries: Vec<(Rgb, u64)> = hist.into_iter().collect();
    entries.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));

    if entries.len() <= max_colors {
        return Palette {
            colors: entries.into_iter().map(|e| e.0).collect(),
        };
    }

    let mut boxes = vec![entries];
    while boxes.len() < max_colors {
        // Dziel pudełko o największym (zakres kanału × liczba pikseli).
        let Some((idx, channel)) = boxes
            .iter()
            .enumerate()
            .filter(|(_, b)| b.len() > 1)
            .map(|(i, b)| {
                let (ch, range) = widest_channel(b);
                let pop: u64 = b.iter().map(|e| e.1).sum();
                (i, ch, range as u64 * pop)
            })
            .max_by(|a, b| a.2.cmp(&b.2).then(b.0.cmp(&a.0)))
            .map(|(i, ch, _)| (i, ch))
        else {
            break;
        };

        let mut b = boxes.swap_remove(idx);
        b.sort_by(|x, y| x.0[channel].cmp(&y.0[channel]).then(x.0.cmp(&y.0)));
        let total: u64 = b.iter().map(|e| e.1).sum();
        let mut acc = 0;
        let mut split = 1;
        for (i, e) in b.iter().enumerate() {
            acc += e.1;
            if acc * 2 >= total {
                split = (i + 1).clamp(1, b.len() - 1);
                break;
            }
        }
        let rest = b.split_off(split);
        boxes.push(b);
        boxes.push(rest);
    }

    let mut colors: Vec<(Rgb, u64)> = boxes
        .iter()
        .map(|b| {
            let pop: u64 = b.iter().map(|e| e.1).sum();
            let mut sum = [0u64; 3];
            for (c, n) in b {
                for k in 0..3 {
                    sum[k] += c[k] as u64 * n;
                }
            }
            let avg = [0, 1, 2].map(|k| ((sum[k] + pop / 2) / pop) as u8);
            (avg, pop)
        })
        .collect();
    colors.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    colors.dedup_by(|a, b| a.0 == b.0);
    Palette {
        colors: colors.into_iter().map(|c| c.0).collect(),
    }
}

fn widest_channel(entries: &[(Rgb, u64)]) -> (usize, u8) {
    let mut best = (0, 0);
    for ch in 0..3 {
        let min = entries.iter().map(|e| e.0[ch]).min().unwrap_or(0);
        let max = entries.iter().map(|e| e.0[ch]).max().unwrap_or(0);
        if max - min > best.1 {
            best = (ch, max - min);
        }
    }
    best
}

fn nearest(palette: &Palette, c: Rgb) -> u8 {
    let dist = |p: &Rgb| -> i32 {
        (0..3)
            .map(|k| {
                let d = p[k] as i32 - c[k] as i32;
                d * d
            })
            .sum()
    };
    palette
        .colors
        .iter()
        .enumerate()
        .min_by_key(|(_, p)| dist(p))
        .map(|(i, _)| i as u8)
        .unwrap_or(0)
}

/// Przypisuje każdemu pikselowi najbliższy kolor palety.
pub fn quantize(img: &RgbaImage, palette: &Palette, alpha_threshold: u8) -> IndexedImage {
    let mut cache: HashMap<Rgb, u8> = HashMap::new();
    let pixels = img
        .pixels()
        .map(|p| {
            if !is_opaque(p[3], alpha_threshold) || palette.colors.is_empty() {
                return None;
            }
            let c = [p[0], p[1], p[2]];
            Some(*cache.entry(c).or_insert_with(|| nearest(palette, c)))
        })
        .collect();
    IndexedImage {
        width: img.width(),
        height: img.height(),
        pixels,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::Rgba;

    fn img_from(w: u32, h: u32, f: impl Fn(u32, u32) -> [u8; 4]) -> RgbaImage {
        RgbaImage::from_fn(w, h, |x, y| Rgba(f(x, y)))
    }

    #[test]
    fn exact_colors_are_kept_when_under_limit() {
        let img = img_from(4, 1, |x, _| match x {
            0 | 1 => [255, 0, 0, 255],
            2 => [0, 255, 0, 255],
            _ => [0, 0, 0, 0],
        });
        let pal = build_palette(std::slice::from_ref(&img), 8, 128);
        assert_eq!(pal.colors, vec![[255, 0, 0], [0, 255, 0]]);
        let idx = quantize(&img, &pal, 128);
        assert_eq!(idx.pixels, vec![Some(0), Some(0), Some(1), None]);
        assert_eq!(idx.opaque_count(), 3);
    }

    #[test]
    fn reduces_to_requested_count() {
        let img = img_from(16, 16, |x, y| [(x * 16) as u8, (y * 16) as u8, 100, 255]);
        let pal = build_palette(std::slice::from_ref(&img), 5, 128);
        assert!(pal.colors.len() <= 5 && !pal.colors.is_empty());
        let idx = quantize(&img, &pal, 128);
        assert!(
            idx.pixels
                .iter()
                .all(|p| p.unwrap() < pal.colors.len() as u8)
        );
    }

    #[test]
    fn palette_is_deterministic() {
        let img = img_from(16, 16, |x, y| {
            [(x * 13) as u8, (y * 7) as u8, (x * y) as u8, 255]
        });
        let a = build_palette(std::slice::from_ref(&img), 6, 128);
        let b = build_palette(&[img], 6, 128);
        assert_eq!(a, b);
    }
}
