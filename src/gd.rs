//! Zamiana prostokątów na obiekty Geometry Dash i budowa poziomu przez gdlib.
//!
//! Źródła wszystkich ID i numerów właściwości są opisane w README (sekcja „Skąd pochodzą wartości”).

use std::path::Path;

use anyhow::{Context, Result, bail};
use gdlib::cclocallevels::{
    gdlevel::{
        GDLevel,
        leveldata::{ColourString, GDLevelHeaderKey, GDLevelHeaderValue, PlayerColour},
        version::GDVersion,
    },
    gdobj::{
        GDObject,
        ids::{objects::COLORED_INNER_SQUARE, properties::Z_LAYER},
        meta::{GDObjAttributes, GDObjConfig},
        structs::{Colour, ColourChannel, GDValue, Group, ZLayer},
    },
};
use serde::{Deserialize, Serialize};

use crate::{mesh::Rect, palette::Palette};

/// Obiekt bazowy pojedynczego "piksela": ID 211, „Colored Inner Square” (gdlib `ids::objects`).
pub const PIXEL_OBJECT_ID: i32 = COLORED_INNER_SQUARE;
/// Rozmiar jednego bloku (kratki edytora) w jednostkach GD przy skali 1.0.
pub const BLOCK_UNITS: f64 = 30.0;
/// Najwyższy zwykły kanał koloru (kanały ≥1000 są specjalne: BG, G1, Obj…).
pub const MAX_COLOR_CHANNEL: i16 = 999;
/// Najwyższe ID grupy w edytorze GD 2.2.
pub const MAX_GROUP: i16 = 9999;
/// Klucz nagłówka poziomu z listą kanałów kolorów (`kS38`).
const COLOURS_HEADER: GDLevelHeaderKey = GDLevelHeaderKey::S(38);

/// Warstwy Z dostępne w edytorze 2.2. Wartości liczbowe jak w gdlib `structs::ZLayer`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ZLayerChoice {
    B5,
    B4,
    B3,
    B2,
    B1,
    T1,
    T2,
    T3,
    T4,
}

impl ZLayerChoice {
    pub const ALL: [ZLayerChoice; 9] = [
        Self::B5,
        Self::B4,
        Self::B3,
        Self::B2,
        Self::B1,
        Self::T1,
        Self::T2,
        Self::T3,
        Self::T4,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::B5 => "B5",
            Self::B4 => "B4",
            Self::B3 => "B3",
            Self::B2 => "B2",
            Self::B1 => "B1",
            Self::T1 => "T1",
            Self::T2 => "T2",
            Self::T3 => "T3",
            Self::T4 => "T4",
        }
    }

    fn to_gd(self) -> ZLayer {
        match self {
            Self::B5 => ZLayer::B5,
            Self::B4 => ZLayer::B4,
            Self::B3 => ZLayer::B3,
            Self::B2 => ZLayer::B2,
            Self::B1 => ZLayer::B1,
            Self::T1 => ZLayer::T1,
            Self::T2 => ZLayer::T2,
            Self::T3 => ZLayer::T3,
            Self::T4 => ZLayer::T4,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ExportSettings {
    pub level_name: String,
    /// Rozmiar jednego piksela PNG w jednostkach GD (30 = cały blok).
    pub pixel_size: f64,
    /// Lewy dolny róg obrazu w jednostkach GD.
    pub origin_x: f64,
    pub origin_y: f64,
    /// Pierwszy kanał koloru; kolor palety nr i dostaje kanał `channel_start + i`.
    pub channel_start: i16,
    /// Pierwsza grupa; klatka nr i dostaje grupę `group_start + i`.
    pub group_start: i16,
    pub z_layer: ZLayerChoice,
    pub z_order: i32,
    /// Warstwa edytora (nie wpływa na grę, ułatwia zaznaczanie).
    pub editor_layer: i16,
    /// No Touch: obiekty nie kolidują z graczem.
    pub no_touch: bool,
    /// Don't Fade + Don't Enter: piksele ignorują efekty wejścia/zanikania.
    pub no_enter_effects: bool,
    /// Dodatkowe powiększenie każdego obiektu w jednostkach GD (na wypadek szpar między blokami).
    pub overlap: f64,
}

impl Default for ExportSettings {
    fn default() -> Self {
        Self {
            level_name: "GDmation".into(),
            pixel_size: 7.5,
            origin_x: 150.0,
            origin_y: 90.0,
            channel_start: 500,
            group_start: 500,
            z_layer: ZLayerChoice::T1,
            z_order: 0,
            editor_layer: 0,
            no_touch: true,
            no_enter_effects: true,
            overlap: 0.0,
        }
    }
}

impl ExportSettings {
    /// Sprawdza, czy zakresy kanałów i grup mieszczą się w limitach GD.
    pub fn validate(&self, palette_len: usize, frame_count: usize) -> Result<()> {
        if self.pixel_size.is_nan() || self.pixel_size <= 0.0 {
            bail!("Rozmiar piksela musi być większy od zera.");
        }
        let last_channel = self.channel_start as i32 + palette_len as i32 - 1;
        if self.channel_start < 1 || last_channel > MAX_COLOR_CHANNEL as i32 {
            bail!(
                "Kanały kolorów {}–{} wychodzą poza zakres 1–{}.",
                self.channel_start,
                last_channel,
                MAX_COLOR_CHANNEL
            );
        }
        let last_group = self.group_start as i32 + frame_count as i32 - 1;
        if self.group_start < 1 || last_group > MAX_GROUP as i32 {
            bail!(
                "Grupy {}–{} wychodzą poza zakres 1–{}.",
                self.group_start,
                last_group,
                MAX_GROUP
            );
        }
        Ok(())
    }

    pub fn channel_for(&self, color: u8) -> i16 {
        self.channel_start + color as i16
    }

    pub fn group_for(&self, frame: usize) -> i16 {
        self.group_start + frame as i16
    }
}

/// Zamienia prostokąty jednej klatki na obiekty GD.
///
/// `image_height` jest potrzebne, bo w PNG oś Y rośnie w dół, a w GD w górę.
/// Pozycja obiektu w GD to jego środek.
pub fn frame_objects(
    rects: &[Rect],
    image_height: u32,
    frame: usize,
    settings: &ExportSettings,
) -> Vec<GDObject> {
    let px = settings.pixel_size;
    let group = Group::Regular(settings.group_for(frame));
    rects
        .iter()
        .map(|r| {
            let cx = settings.origin_x + (r.x as f64 + r.w as f64 / 2.0) * px;
            let cy = settings.origin_y + (image_height as f64 - r.y as f64 - r.h as f64 / 2.0) * px;
            let sx = (r.w as f64 * px + settings.overlap) / BLOCK_UNITS;
            let sy = (r.h as f64 * px + settings.overlap) / BLOCK_UNITS;

            let mut config = GDObjConfig::new()
                .with_pos(cx, cy)
                .with_scale(sx, sy)
                .with_groups([group])
                .with_z_layer(settings.z_layer.to_gd())
                .with_z_order(settings.z_order)
                .with_base_colour(ColourChannel::Channel(settings.channel_for(r.color)))
                .with_attributes(GDObjAttributes::no_touch, settings.no_touch)
                .with_attributes(GDObjAttributes::dont_fade, settings.no_enter_effects)
                .with_attributes(GDObjAttributes::dont_enter, settings.no_enter_effects);
            config.editor_layers = (settings.editor_layer, 0);
            // gdlib pomija klucz 24, gdy warstwa = T1 (uznaje ją za domyślną). W grze brak klucza
            // oznacza domyślną warstwę obiektu, więc T1 zapisujemy jawnie.
            let extra = if settings.z_layer == ZLayerChoice::T1 {
                vec![(Z_LAYER, GDValue::Int(ZLayer::T1 as i32))]
            } else {
                vec![]
            };
            GDObject::new(PIXEL_OBJECT_ID, &config, extra)
        })
        .collect()
}

/// Wpis kanału koloru w nagłówku `kS38`, wzorowany na domyślnych wpisach gdlib
/// (`DEFAULT_LEVEL_HEADERS`): pełna nieprzezroczystość, brak kopiowania koloru gracza.
fn colour_entry(channel: i16, rgb: [u8; 3]) -> ColourString {
    ColourString {
        from: Colour::from_rgb((rgb[0], rgb[1], rgb[2])),
        to: Colour::from_rgb((255, 255, 255)),
        player_colour: PlayerColour::None,
        colour_ch_idx: channel as i32,
        from_opacity: 1.0,
        to_opacity: 1.0,
        opacity_toggled: true,
        ..Default::default()
    }
}

/// Jedna klatka gotowa do eksportu.
pub struct FrameMesh<'a> {
    pub rects: &'a [Rect],
    pub height: u32,
}

/// Buduje nowy poziom (format 2.209) zawierający wszystkie klatki i kanały kolorów palety.
pub fn build_level(
    frames: &[FrameMesh],
    palette: &Palette,
    settings: &ExportSettings,
) -> Result<GDLevel> {
    settings.validate(palette.colors.len(), frames.len())?;

    let mut level = GDLevel::new(GDVersion::GD2209);
    level.identity.name = settings.level_name.clone();
    level.identity.game_version = GDVersion::GD2209.to_game_version_id().unwrap_or(22);

    let objects: Vec<GDObject> = frames
        .iter()
        .enumerate()
        .flat_map(|(i, f)| frame_objects(f.rects, f.height, i, settings))
        .collect();
    level.content.object_count = objects.len() as i32;

    let data = level
        .get_decrypted_data_ref()
        .context("Nowy poziom nie ma danych (błąd gdlib).")?;
    data.objects = objects;

    let entry = data
        .headers
        .entry(COLOURS_HEADER)
        .or_insert_with(|| GDLevelHeaderValue::ColourString(vec![]));
    let GDLevelHeaderValue::ColourString(colours) = entry else {
        bail!("Nagłówek kS38 ma nieoczekiwany typ.");
    };
    for (i, rgb) in palette.colors.iter().enumerate() {
        let ch = settings.channel_for(i as u8);
        colours.retain(|c| c.colour_ch_idx != ch as i32);
        colours.push(colour_entry(ch, *rgb));
    }

    Ok(level)
}

pub fn export_gmd(level: &GDLevel, path: &Path) -> Result<()> {
    level
        .export_to_gmd(path)
        .map_err(|e| anyhow::anyhow!("{e:?}"))
        .with_context(|| format!("Nie udało się zapisać {}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mesh::greedy_mesh;
    use crate::palette::IndexedImage;

    fn sample() -> (Vec<Rect>, Palette) {
        // 4x2: górny wiersz czerwony, dolny: zielony, zielony, przezroczysty, czerwony
        let img = IndexedImage {
            width: 4,
            height: 2,
            pixels: vec![
                Some(0),
                Some(0),
                Some(0),
                Some(0),
                Some(1),
                Some(1),
                None,
                Some(0),
            ],
        };
        let palette = Palette {
            colors: vec![[200, 0, 0], [0, 180, 20]],
        };
        (greedy_mesh(&img), palette)
    }

    #[test]
    fn object_geometry() {
        let settings = ExportSettings {
            pixel_size: 7.5,
            origin_x: 0.0,
            origin_y: 0.0,
            ..Default::default()
        };
        let rect = Rect {
            x: 1,
            y: 0,
            w: 2,
            h: 1,
            color: 1,
        };
        let obj = &frame_objects(&[rect], 2, 0, &settings)[0];
        assert_eq!(obj.id, 211);
        // środek: x = (1 + 1) * 7.5 = 15, y = (2 - 0 - 0.5) * 7.5 = 11.25
        assert_eq!(obj.config.pos, (15.0, 11.25));
        assert_eq!(obj.config.scale, (0.5, 0.25));
        assert_eq!(obj.config.colour_channels.0, ColourChannel::Channel(501));
        assert_eq!(obj.config.groups, vec![Group::Regular(500)]);
        assert!(obj.config.get_attributes(GDObjAttributes::no_touch));
    }

    #[test]
    fn validation_catches_ranges() {
        let s = ExportSettings {
            channel_start: 998,
            ..Default::default()
        };
        assert!(s.validate(2, 1).is_ok());
        assert!(s.validate(3, 1).is_err());
        let s = ExportSettings {
            group_start: 9999,
            ..Default::default()
        };
        assert!(s.validate(1, 1).is_ok());
        assert!(s.validate(1, 2).is_err());
    }

    /// Round-trip: zapis .gmd przez gdlib i ponowne wczytanie.
    #[test]
    fn gmd_round_trip() {
        let (rects, palette) = sample();
        let settings = ExportSettings {
            level_name: "Test krew".into(),
            ..Default::default()
        };
        let level = build_level(
            &[FrameMesh {
                rects: &rects,
                height: 2,
            }],
            &palette,
            &settings,
        )
        .unwrap();

        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.gmd");
        export_gmd(&level, &path).unwrap();

        let mut loaded = GDLevel::from_gmd(&path).unwrap();
        assert_eq!(loaded.identity.name, "Test krew");
        assert_eq!(loaded.identity.binary_version, GDVersion::GD2209);

        // gdlib 0.4.3 przy odczycie parsuje "1" jako bool przez `str::parse`, więc flagi
        // (No Touch, Don't Fade...) giną w sparsowanym obiekcie. Sprawdzamy je w surowym stringu.
        let raw = loaded.get_raw_decrypted_data().unwrap();
        let raw_objects: Vec<&str> = raw.split(';').skip(1).filter(|o| !o.is_empty()).collect();
        assert_eq!(raw_objects.len(), rects.len());
        for o in &raw_objects {
            let kv: Vec<&str> = o.split(',').collect();
            let has = |k: &str, v: &str| kv.chunks(2).any(|p| p == [k, v]);
            assert!(has("1", "211"), "{o}");
            assert!(has("121", "1") && has("64", "1") && has("67", "1"), "{o}");
            assert!(has("24", "5"), "Z-layer T1 musi być zapisany jawnie: {o}");
            assert_eq!(
                kv.iter().step_by(2).filter(|k| **k == "24").count(),
                1,
                "{o}"
            );
        }

        let data = loaded.get_decrypted_data_ref().unwrap();
        assert_eq!(data.objects.len(), rects.len());

        let original = frame_objects(&rects, 2, 0, &settings);
        for (a, b) in original.iter().zip(&data.objects) {
            assert_eq!(a.id, b.id);
            assert_eq!(a.config.pos, b.config.pos);
            assert_eq!(a.config.scale, b.config.scale);
            assert_eq!(a.config.colour_channels.0, b.config.colour_channels.0);
            assert_eq!(a.config.groups, b.config.groups);
            assert_eq!(a.config.z_layer, b.config.z_layer);
        }

        let Some(GDLevelHeaderValue::ColourString(colours)) = data.headers.get(&COLOURS_HEADER)
        else {
            panic!("brak kS38");
        };
        for (i, rgb) in palette.colors.iter().enumerate() {
            let c = colours
                .iter()
                .find(|c| c.colour_ch_idx == 500 + i as i32)
                .expect("brak kanału palety");
            assert_eq!(
                (c.from.red, c.from.green, c.from.blue),
                (rgb[0], rgb[1], rgb[2])
            );
        }
        // domyślne kanały gdlib (BG, G1, ...) nadal obecne
        assert!(colours.iter().any(|c| c.colour_ch_idx == 1000));
    }
}
