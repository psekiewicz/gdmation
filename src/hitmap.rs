//! Mapa hitboxów: wczytanie poziomu `.gmd`, podział obiektów na kategorie i ich kształty.
//!
//! Gra nie udostępnia w plikach tabeli „ID obiektu → hitbox” (sprawdzone na instalacji Steam),
//! więc kategoria jest *proponowana* z nazwy obiektu (tabela `OBJECT_NAMES` z gdlib), a autor
//! poziomu ją zatwierdza lub poprawia. Zasada autora: zakładka Hazards = czerwony hitbox,
//! Blocks = hitbox bloku (pół blok osobno), Slopes = slope jedno- albo dwublokowy.
//!
//! Kształty to obrys obiektu w kratce 30×30 (po skali, odbiciu i obrocie), nie dokładny hitbox gry.

use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::Path,
};

use anyhow::{Context, Result};
use gdlib::cclocallevels::{gdlevel::GDLevel, properties::OBJECT_NAMES};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Category {
    Block,
    HalfBlock,
    Hazard,
    SlopeShort,
    SlopeLong,
    NoHitbox,
}

impl Category {
    pub const ALL: [Category; 6] = [
        Self::Block,
        Self::HalfBlock,
        Self::Hazard,
        Self::SlopeShort,
        Self::SlopeLong,
        Self::NoHitbox,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::Block => "Blok",
            Self::HalfBlock => "Pół blok",
            Self::Hazard => "Zagrożenie (kolec)",
            Self::SlopeShort => "Slope 1 blok",
            Self::SlopeLong => "Slope 2 bloki",
            Self::NoHitbox => "Bez hitboxu",
        }
    }
}

/// Fragmenty nazw obiektów bez hitboxu gracza (dekoracje, triggery, podglądy edytora).
const NO_HITBOX_WORDS: &[&str] = &[
    "decorative",
    "deco ",
    "3dl",
    "outline",
    "fake",
    "pixel",
    "slope corner",
    "connector",
    "filler",
    "spike base",
    "glow",
    "particle",
    "trigger",
    "portal",
    "pad",
    "orb",
    "coin",
    "text",
    "line",
    "animated",
    "animation",
    "monster",
    "ring",
    "arrow",
    "cloud",
    "bush",
    "chain",
    "light",
    "background",
    "ground",
    "collision",
    "overlay",
    "pulsing",
    "template",
];

pub fn object_name(id: i32) -> Option<&'static str> {
    OBJECT_NAMES.get(&id).copied()
}

/// Propozycja kategorii na podstawie nazwy obiektu. Do zatwierdzenia przez autora poziomu.
pub fn suggest(id: i32) -> Category {
    let Some(name) = object_name(id) else {
        return Category::NoHitbox;
    };
    let l = name.to_lowercase();
    if NO_HITBOX_WORDS.iter().any(|w| l.contains(w)) {
        Category::NoHitbox
    } else if ["hazard", "spike", "saw", "blade"]
        .iter()
        .any(|w| l.contains(w))
    {
        Category::Hazard
    } else if l.contains("slope") {
        if l.contains("wide") {
            Category::SlopeLong
        } else {
            Category::SlopeShort
        }
    } else if l.contains("slab") {
        Category::HalfBlock
    } else if ["square", "block", "brick"].iter().any(|w| l.contains(w)) {
        Category::Block
    } else {
        Category::NoHitbox
    }
}

/// Propozycja odwrócenia slope'a względem kształtu domyślnego (pełna część w prawym dolnym rogu).
/// Nazwy typu „Bottom-Left Slope” sugerują lustrzane odbicie. Do sprawdzenia w grze.
pub fn suggest_slope_mirror(id: i32) -> bool {
    object_name(id).is_some_and(|n| n.contains("Bottom-Left"))
}

/// Obiekt poziomu z polami potrzebnymi do mapy.
#[derive(Debug, Clone, PartialEq)]
pub struct MapObject {
    pub id: i32,
    pub x: f64,
    pub y: f64,
    /// Obrót w stopniach (właściwość 6).
    pub rotation: f64,
    pub flip_x: bool,
    pub flip_y: bool,
    pub scale_x: f64,
    pub scale_y: f64,
    /// No Touch (121) albo Passable (134): obiekt nie koliduje z graczem.
    pub no_collision: bool,
}

fn flag(v: &str) -> bool {
    v == "1" || v == "true"
}

/// Parsuje surowy, zdekodowany string poziomu (`nagłówek;obiekt;obiekt;...`).
///
/// Własny parser zamiast `GDObject::parse_str`, bo gdlib 0.4.3 czyta flagi `1` jako `false`.
/// Numery właściwości wg gdlib `PROPERTY_TABLE`: 1 ID, 2/3 pozycja, 4/5 odbicie X/Y, 6 obrót,
/// 32 skala, 128/129 skala X/Y, 121 No Touch, 134 Passable.
pub fn parse_objects(raw: &str) -> Vec<MapObject> {
    raw.split(';')
        .skip(1)
        .filter(|o| !o.is_empty())
        .filter_map(|o| {
            let mut obj = MapObject {
                id: 0,
                x: 0.0,
                y: 0.0,
                rotation: 0.0,
                flip_x: false,
                flip_y: false,
                scale_x: 1.0,
                scale_y: 1.0,
                no_collision: false,
            };
            let mut scale = 1.0;
            let mut it = o.split(',');
            while let (Some(k), Some(v)) = (it.next(), it.next()) {
                match k {
                    "1" => obj.id = v.parse().ok()?,
                    "2" => obj.x = v.parse().unwrap_or(0.0),
                    "3" => obj.y = v.parse().unwrap_or(0.0),
                    "4" => obj.flip_x = flag(v),
                    "5" => obj.flip_y = flag(v),
                    "6" => obj.rotation = v.parse().unwrap_or(0.0),
                    "32" => scale = v.parse().unwrap_or(1.0),
                    "128" => obj.scale_x = v.parse().unwrap_or(1.0),
                    "129" => obj.scale_y = v.parse().unwrap_or(1.0),
                    "121" | "134" => obj.no_collision |= flag(v),
                    _ => {}
                }
            }
            obj.scale_x *= scale;
            obj.scale_y *= scale;
            (obj.id != 0).then_some(obj)
        })
        .collect()
}

pub struct LoadedLevel {
    pub name: String,
    pub objects: Vec<MapObject>,
}

pub fn load_gmd(path: &Path) -> Result<LoadedLevel> {
    let level = GDLevel::from_gmd(path)
        .map_err(|e| anyhow::anyhow!("{e:?}"))
        .with_context(|| format!("Nie można wczytać {}", path.display()))?;
    let raw = level
        .get_raw_decrypted_data()
        .context("Poziom nie zawiera danych obiektów.")?;
    Ok(LoadedLevel {
        name: level.identity.name.clone(),
        objects: parse_objects(&raw),
    })
}

/// Kształt w jednostkach GD (oś Y w górę), gotowy do narysowania.
#[derive(Debug, Clone, PartialEq)]
pub struct Shape {
    pub category: Category,
    pub points: Vec<(f64, f64)>,
}

/// Obrys kategorii w układzie lokalnym obiektu (środek = 0,0, kratka 30×30).
fn local_outline(cat: Category, mirror: bool) -> Option<Vec<(f64, f64)>> {
    let pts = match cat {
        Category::Block | Category::Hazard => {
            vec![(-15.0, -15.0), (15.0, -15.0), (15.0, 15.0), (-15.0, 15.0)]
        }
        // Pół blok w górnej połowie kratki: do sprawdzenia w grze.
        Category::HalfBlock => vec![(-15.0, 0.0), (15.0, 0.0), (15.0, 15.0), (-15.0, 15.0)],
        Category::SlopeShort => vec![(-15.0, -15.0), (15.0, -15.0), (15.0, 15.0)],
        Category::SlopeLong => vec![(-30.0, -15.0), (30.0, -15.0), (30.0, 15.0)],
        Category::NoHitbox => return None,
    };
    Some(if mirror {
        pts.into_iter().map(|(x, y)| (-x, y)).collect()
    } else {
        pts
    })
}

/// Skala → odbicie → obrót (zgodnie z ruchem wskazówek zegara, jak w cocos2d) → przesunięcie.
pub fn shape(obj: &MapObject, cat: Category, mirror: bool) -> Option<Shape> {
    let (sin, cos) = obj.rotation.to_radians().sin_cos();
    let fx = if obj.flip_x { -1.0 } else { 1.0 };
    let fy = if obj.flip_y { -1.0 } else { 1.0 };
    let points = local_outline(cat, mirror)?
        .into_iter()
        .map(|(x, y)| {
            let (x, y) = (x * obj.scale_x * fx, y * obj.scale_y * fy);
            (obj.x + x * cos + y * sin, obj.y - x * sin + y * cos)
        })
        .collect();
    Some(Shape {
        category: cat,
        points,
    })
}

/// Kategorie zatwierdzone lub poprawione przez autora; zapamiętywane między uruchomieniami.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct CategoryStore {
    pub overrides: BTreeMap<i32, Category>,
    pub slope_mirror: BTreeMap<i32, bool>,
    pub confirmed: BTreeSet<i32>,
}

impl CategoryStore {
    pub fn category(&self, id: i32) -> Category {
        self.overrides
            .get(&id)
            .copied()
            .unwrap_or_else(|| suggest(id))
    }

    pub fn mirror(&self, id: i32) -> bool {
        self.slope_mirror
            .get(&id)
            .copied()
            .unwrap_or_else(|| suggest_slope_mirror(id))
    }

    pub fn load(path: &Path) -> Self {
        fs::read_to_string(path)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        if let Some(dir) = path.parent() {
            fs::create_dir_all(dir)?;
        }
        fs::write(path, serde_json::to_string_pretty(self)?)?;
        Ok(())
    }
}

/// Użyte ID z liczbą wystąpień, posortowane malejąco po liczbie.
pub fn used_ids(objects: &[MapObject]) -> Vec<(i32, usize)> {
    let mut counts: BTreeMap<i32, usize> = BTreeMap::new();
    for o in objects {
        *counts.entry(o.id).or_insert(0) += 1;
    }
    let mut v: Vec<_> = counts.into_iter().collect();
    v.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    v
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn suggestions_follow_names() {
        assert_eq!(suggest(211), Category::Block); // Colored Inner Square
        assert_eq!(suggest(215), Category::HalfBlock); // Colored Slab
        assert_eq!(suggest(216), Category::Hazard); // Colored Spike
        assert_eq!(suggest(144), Category::Hazard); // Invisible Spike
        assert_eq!(suggest(363), Category::Hazard); // Spike Black Slope Hazard
        assert_eq!(suggest(1743), Category::SlopeShort); // Grid Patterned Slope
        assert_eq!(suggest(1744), Category::SlopeLong); // Grid Patterned Wide Slope
        assert_eq!(suggest(18), Category::NoHitbox); // Large Decorative Spikes
        assert_eq!(suggest(191), Category::NoHitbox); // Fake Black Spike
        assert_eq!(suggest(586), Category::NoHitbox); // Colored 3DL Slope
        assert_eq!(suggest(1816), Category::NoHitbox); // Collision Block
        assert_eq!(suggest(162), Category::Block); // Solid Stripe Outer Corner Square
        assert_eq!(suggest(373), Category::NoHitbox); // Black Gradient Slope Corner
        assert_eq!(suggest(-5), Category::NoHitbox);
        assert!(suggest_slope_mirror(1770)); // Jagged Rocks 3 Bottom-Left Slope
        assert!(!suggest_slope_mirror(1769));
    }

    #[test]
    fn parses_raw_objects() {
        let raw = "kS38,1_0;1,1,2,15,3,45,6,90,121,1;1,1743,2,75,3,15,4,1,32,2,128,0.5;;";
        let objs = parse_objects(raw);
        assert_eq!(objs.len(), 2);
        assert_eq!(
            (objs[0].id, objs[0].x, objs[0].y, objs[0].rotation),
            (1, 15.0, 45.0, 90.0)
        );
        assert!(objs[0].no_collision);
        assert!(objs[1].flip_x && !objs[1].no_collision);
        assert_eq!((objs[1].scale_x, objs[1].scale_y), (1.0, 2.0));
    }

    fn obj(x: f64, y: f64) -> MapObject {
        MapObject {
            id: 1,
            x,
            y,
            rotation: 0.0,
            flip_x: false,
            flip_y: false,
            scale_x: 1.0,
            scale_y: 1.0,
            no_collision: false,
        }
    }

    fn approx(a: &[(f64, f64)], b: &[(f64, f64)]) {
        assert_eq!(a.len(), b.len());
        for (p, q) in a.iter().zip(b) {
            assert!(
                (p.0 - q.0).abs() < 1e-9 && (p.1 - q.1).abs() < 1e-9,
                "{a:?} != {b:?}"
            );
        }
    }

    #[test]
    fn shapes_are_transformed() {
        let o = obj(100.0, 50.0);
        let s = shape(&o, Category::SlopeShort, false).unwrap();
        approx(&s.points, &[(85.0, 35.0), (115.0, 35.0), (115.0, 65.0)]);

        let s = shape(&o, Category::SlopeLong, true).unwrap();
        approx(&s.points, &[(130.0, 35.0), (70.0, 35.0), (70.0, 65.0)]);

        // obrót o 90° zgodnie z ruchem wskazówek zegara: punkt (15,15) → (15,-15)
        let o = MapObject {
            rotation: 90.0,
            ..obj(0.0, 0.0)
        };
        let s = shape(&o, Category::SlopeShort, false).unwrap();
        approx(&s.points, &[(-15.0, 15.0), (-15.0, -15.0), (15.0, -15.0)]);

        let o = MapObject {
            scale_x: 2.0,
            flip_y: true,
            ..obj(0.0, 0.0)
        };
        let s = shape(&o, Category::HalfBlock, false).unwrap();
        approx(
            &s.points,
            &[(-30.0, 0.0), (30.0, 0.0), (30.0, -15.0), (-30.0, -15.0)],
        );

        assert!(shape(&obj(0.0, 0.0), Category::NoHitbox, false).is_none());
    }

    #[test]
    fn store_overrides_suggestions() {
        let mut s = CategoryStore::default();
        assert_eq!(s.category(211), Category::Block);
        s.overrides.insert(211, Category::NoHitbox);
        s.slope_mirror.insert(1743, true);
        assert_eq!(s.category(211), Category::NoHitbox);
        assert!(s.mirror(1743));

        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("k.json");
        s.save(&path).unwrap();
        assert_eq!(CategoryStore::load(&path), s);
    }

    #[test]
    fn loads_level_with_real_objects() {
        // poziom zbudowany przez GDmation: wszystkie obiekty to 211 z No Touch
        use crate::{gd, mesh::Rect, palette::Palette};
        let rects = [Rect {
            x: 0,
            y: 0,
            w: 2,
            h: 1,
            color: 0,
        }];
        let palette = Palette {
            colors: vec![[255, 0, 0]],
        };
        let level = gd::build_level(
            &[gd::FrameMesh {
                rects: &rects,
                height: 1,
            }],
            &palette,
            &gd::ExportSettings::default(),
        )
        .unwrap();
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("l.gmd");
        gd::export_gmd(&level, &path).unwrap();

        let loaded = load_gmd(&path).unwrap();
        assert_eq!(loaded.objects.len(), 1);
        assert_eq!(loaded.objects[0].id, 211);
        assert!(loaded.objects[0].no_collision);
        assert_eq!(used_ids(&loaded.objects), vec![(211, 1)]);
    }
}
