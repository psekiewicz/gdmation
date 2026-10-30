//! Greedy meshing: scalanie sąsiednich pikseli tego samego koloru w prostokąty.
//!
//! Każdy prostokąt staje się jednym obiektem GD z odpowiednią skalą X/Y.

use crate::palette::IndexedImage;

/// Prostokąt w pikselach obrazu; (x, y) = lewy górny róg, oś Y w dół (jak w PNG).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rect {
    pub x: u32,
    pub y: u32,
    pub w: u32,
    pub h: u32,
    pub color: u8,
}

/// Zwraca pokrycie obrazu prostokątami. Uruchamia scalanie wierszami i kolumnami,
/// wybiera wariant z mniejszą liczbą obiektów.
pub fn greedy_mesh(img: &IndexedImage) -> Vec<Rect> {
    let rows = mesh_rows(img.width, img.height, |x, y| img.get(x, y));
    let cols = mesh_rows(img.height, img.width, |x, y| img.get(y, x));
    if cols.len() < rows.len() {
        cols.into_iter()
            .map(|r| Rect {
                x: r.y,
                y: r.x,
                w: r.h,
                h: r.w,
                color: r.color,
            })
            .collect()
    } else {
        rows
    }
}

/// Klasyczny greedy: najpierw rozszerzaj w prawo, potem w dół, dopóki cały wiersz pasuje.
fn mesh_rows(w: u32, h: u32, get: impl Fn(u32, u32) -> Option<u8>) -> Vec<Rect> {
    let mut used = vec![false; (w * h) as usize];
    let idx = |x: u32, y: u32| (y * w + x) as usize;
    let mut out = Vec::new();

    for y in 0..h {
        let mut x = 0;
        while x < w {
            let Some(color) = get(x, y).filter(|_| !used[idx(x, y)]) else {
                x += 1;
                continue;
            };

            let mut rw = 1;
            while x + rw < w && !used[idx(x + rw, y)] && get(x + rw, y) == Some(color) {
                rw += 1;
            }

            let mut rh = 1;
            'grow: while y + rh < h {
                for dx in 0..rw {
                    if used[idx(x + dx, y + rh)] || get(x + dx, y + rh) != Some(color) {
                        break 'grow;
                    }
                }
                rh += 1;
            }

            for dy in 0..rh {
                for dx in 0..rw {
                    used[idx(x + dx, y + dy)] = true;
                }
            }
            out.push(Rect {
                x,
                y,
                w: rw,
                h: rh,
                color,
            });
            x += rw;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn indexed(w: u32, h: u32, pixels: Vec<Option<u8>>) -> IndexedImage {
        IndexedImage {
            width: w,
            height: h,
            pixels,
        }
    }

    /// Każdy nieprzezroczysty piksel pokryty dokładnie raz, kolorem zgodnym z obrazem.
    fn assert_exact_cover(img: &IndexedImage, rects: &[Rect]) {
        let mut cover = vec![0u8; img.pixels.len()];
        for r in rects {
            for y in r.y..r.y + r.h {
                for x in r.x..r.x + r.w {
                    assert_eq!(img.get(x, y), Some(r.color), "zły kolor w ({x},{y})");
                    cover[(y * img.width + x) as usize] += 1;
                }
            }
        }
        for (i, p) in img.pixels.iter().enumerate() {
            assert_eq!(cover[i], p.is_some() as u8, "pokrycie piksela {i}");
        }
    }

    #[test]
    fn solid_square_is_one_object() {
        let img = indexed(8, 8, vec![Some(3); 64]);
        let rects = greedy_mesh(&img);
        assert_eq!(
            rects,
            vec![Rect {
                x: 0,
                y: 0,
                w: 8,
                h: 8,
                color: 3
            }]
        );
    }

    #[test]
    fn empty_image_has_no_objects() {
        let img = indexed(4, 4, vec![None; 16]);
        assert!(greedy_mesh(&img).is_empty());
    }

    #[test]
    fn picks_better_orientation() {
        // Dwie pionowe kolumny różnych kolorów: wariant kolumnowy daje 2 obiekty.
        let mut px = Vec::new();
        for _y in 0..5 {
            px.push(Some(0));
            px.push(Some(1));
        }
        let img = indexed(2, 5, px);
        let rects = greedy_mesh(&img);
        assert_eq!(rects.len(), 2);
        assert_exact_cover(&img, &rects);
    }

    #[test]
    fn random_images_are_covered_exactly() {
        let mut seed: u64 = 0x1234_5678;
        let mut next = || {
            seed = seed
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            (seed >> 33) as u32
        };
        for _ in 0..200 {
            let w = next() % 20 + 1;
            let h = next() % 20 + 1;
            let colors = next() % 4 + 1;
            let px = (0..w * h)
                .map(|_| {
                    let v = next() % (colors + 1);
                    (v < colors).then_some(v as u8)
                })
                .collect();
            let img = indexed(w, h, px);
            let rects = greedy_mesh(&img);
            assert_exact_cover(&img, &rects);
            assert!(rects.len() <= img.opaque_count());
        }
    }
}
