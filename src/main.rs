#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

use std::path::PathBuf;

use eframe::egui::{
    self, Color32, ColorImage, Pos2, Rect, Stroke, StrokeKind, TextureHandle, TextureOptions, Vec2,
};
use gdmation::{
    gd::{self, ExportSettings, ZLayerChoice},
    pipeline::{self, Processed, ReduceOptions},
};
use image::RgbaImage;

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("GDmation – animacje pixel-art dla GD 2.209")
            .with_inner_size([1200.0, 800.0]),
        ..Default::default()
    };
    // Ścieżka PNG jako argument (np. plik upuszczony na .exe w Windows).
    let initial = std::env::args_os().nth(1).map(PathBuf::from);
    eframe::run_native(
        "GDmation",
        options,
        Box::new(move |cc| {
            let mut app = App::default();
            if let Some(path) = initial {
                app.load_path(&cc.egui_ctx, path);
            }
            Ok(Box::new(app))
        }),
    )
}

/// Gotowe rozmiary piksela (jednostki GD; 30 = jeden blok edytora).
const PIXEL_PRESETS: [(f64, &str); 5] = [
    (30.0, "1 blok (30)"),
    (15.0, "1/2 bloku (15)"),
    (7.5, "1/4 bloku (7.5)"),
    (3.75, "1/8 bloku (3.75)"),
    (1.875, "1/16 bloku (1.875)"),
];

struct Frame {
    path: PathBuf,
    image: RgbaImage,
}

struct App {
    frame: Option<Frame>,
    reduce: ReduceOptions,
    settings: ExportSettings,

    processed: Option<Processed>,
    processed_for: Option<ReduceOptions>,
    tex_original: Option<TextureHandle>,
    tex_reduced: Option<TextureHandle>,

    zoom: f32,
    show_grid: bool,
    show_rects: bool,
    show_original: bool,
    fit_pending: bool,
    status: String,
}

impl Default for App {
    fn default() -> Self {
        Self {
            frame: None,
            reduce: ReduceOptions::default(),
            settings: ExportSettings::default(),
            processed: None,
            processed_for: None,
            tex_original: None,
            tex_reduced: None,
            zoom: 8.0,
            show_grid: true,
            show_rects: true,
            show_original: false,
            fit_pending: false,
            status: "Wczytaj klatkę PNG, aby zacząć.".into(),
        }
    }
}

fn rgba_to_color_image(img: &RgbaImage) -> ColorImage {
    ColorImage::from_rgba_unmultiplied([img.width() as usize, img.height() as usize], img.as_raw())
}

impl App {
    fn open_png(&mut self, ctx: &egui::Context) {
        if let Some(path) = rfd::FileDialog::new()
            .add_filter("PNG", &["png"])
            .pick_file()
        {
            self.load_path(ctx, path);
        }
    }

    fn load_path(&mut self, ctx: &egui::Context, path: PathBuf) {
        match pipeline::load_png(&path) {
            Ok(image) => {
                self.tex_original = Some(ctx.load_texture(
                    "original",
                    rgba_to_color_image(&image),
                    TextureOptions::NEAREST,
                ));
                self.status = format!(
                    "Wczytano {} ({}×{} px).",
                    path.display(),
                    image.width(),
                    image.height()
                );
                self.frame = Some(Frame { path, image });
                self.fit_pending = true;
                self.processed_for = None;
            }
            Err(e) => self.status = format!("Błąd: {e:#}"),
        }
    }

    /// Przelicza paletę i scalanie, gdy zmieniła się klatka lub opcje redukcji.
    fn refresh(&mut self, ctx: &egui::Context) {
        let Some(frame) = &self.frame else { return };
        if self.processed_for == Some(self.reduce) {
            return;
        }
        let processed = pipeline::process(std::slice::from_ref(&frame.image), self.reduce);
        let f = &processed.frames[0];
        let rgba: Vec<u8> = f
            .indexed
            .pixels
            .iter()
            .flat_map(|p| match p {
                Some(i) => {
                    let c = processed.palette.colors[*i as usize];
                    [c[0], c[1], c[2], 255]
                }
                None => [0, 0, 0, 0],
            })
            .collect();
        let img = ColorImage::from_rgba_unmultiplied(
            [f.indexed.width as usize, f.indexed.height as usize],
            &rgba,
        );
        self.tex_reduced = Some(ctx.load_texture("reduced", img, TextureOptions::NEAREST));
        self.processed = Some(processed);
        self.processed_for = Some(self.reduce);
    }

    fn export_gmd(&mut self) {
        let Some(processed) = &self.processed else {
            return;
        };
        let level = match processed.build_level(&self.settings) {
            Ok(l) => l,
            Err(e) => {
                self.status = format!("Błąd: {e:#}");
                return;
            }
        };
        let Some(path) = rfd::FileDialog::new()
            .add_filter("Poziom GD", &["gmd"])
            .set_file_name(format!("{}.gmd", self.settings.level_name))
            .save_file()
        else {
            return;
        };
        self.status = match gd::export_gmd(&level, &path) {
            Ok(()) => format!(
                "Zapisano {} ({} obiektów).",
                path.display(),
                processed.objects_after()
            ),
            Err(e) => format!("Błąd: {e:#}"),
        };
    }

    fn side_panel(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        egui::ScrollArea::vertical().show(ui, |ui| {
            ui.heading("Klatka");
            if ui.button("Wczytaj PNG…").clicked() {
                self.open_png(&ctx);
            }
            if let Some(f) = &self.frame {
                ui.label(
                    f.path
                        .file_name()
                        .map(|n| n.to_string_lossy().to_string())
                        .unwrap_or_default(),
                );
            }

            ui.separator();
            ui.heading("Paleta");
            ui.add(egui::Slider::new(&mut self.reduce.max_colors, 1..=64).text("maks. kolorów"));
            ui.add(egui::Slider::new(&mut self.reduce.alpha_threshold, 1..=255).text("próg alfy"));
            if let Some(p) = &self.processed {
                ui.label(format!("Kolorów w palecie: {}", p.palette.colors.len()));
                ui.horizontal_wrapped(|ui| {
                    for (i, c) in p.palette.colors.iter().enumerate() {
                        let (rect, resp) =
                            ui.allocate_exact_size(Vec2::splat(18.0), egui::Sense::hover());
                        ui.painter()
                            .rect_filled(rect, 2.0, Color32::from_rgb(c[0], c[1], c[2]));
                        resp.on_hover_text(format!(
                            "Kanał {} – #{:02X}{:02X}{:02X}",
                            self.settings.channel_for(i as u8),
                            c[0],
                            c[1],
                            c[2]
                        ));
                    }
                });

                ui.separator();
                ui.heading("Obiekty");
                let before = p.objects_before();
                let after = p.objects_after();
                ui.label(format!("Przed scaleniem: {before}"));
                ui.label(format!("Po scaleniu: {after}"));
                if before > 0 {
                    ui.label(format!(
                        "Oszczędność: {:.0}%",
                        100.0 * (1.0 - after as f64 / before as f64)
                    ));
                }
            }

            ui.separator();
            ui.heading("Eksport");
            egui::Grid::new("export").num_columns(2).show(ui, |ui| {
                let s = &mut self.settings;
                ui.label("Nazwa poziomu");
                ui.text_edit_singleline(&mut s.level_name);
                ui.end_row();

                ui.label("Rozmiar piksela");
                egui::ComboBox::from_id_salt("pixel")
                    .selected_text(
                        PIXEL_PRESETS
                            .iter()
                            .find(|p| p.0 == s.pixel_size)
                            .map(|p| p.1.to_string())
                            .unwrap_or_else(|| format!("{}", s.pixel_size)),
                    )
                    .show_ui(ui, |ui| {
                        for (v, label) in PIXEL_PRESETS {
                            ui.selectable_value(&mut s.pixel_size, v, label);
                        }
                    });
                ui.end_row();
                ui.label("");
                ui.add(
                    egui::DragValue::new(&mut s.pixel_size)
                        .range(0.1..=300.0)
                        .speed(0.05)
                        .suffix(" j."),
                );
                ui.end_row();

                ui.label("Początek X / Y");
                ui.horizontal(|ui| {
                    ui.add(egui::DragValue::new(&mut s.origin_x).speed(1.0));
                    ui.add(egui::DragValue::new(&mut s.origin_y).speed(1.0));
                });
                ui.end_row();

                ui.label("Pierwszy kanał koloru");
                ui.add(egui::DragValue::new(&mut s.channel_start).range(1..=gd::MAX_COLOR_CHANNEL));
                ui.end_row();

                ui.label("Pierwsza grupa");
                ui.add(egui::DragValue::new(&mut s.group_start).range(1..=gd::MAX_GROUP));
                ui.end_row();

                ui.label("Z-layer");
                egui::ComboBox::from_id_salt("zlayer")
                    .selected_text(s.z_layer.label())
                    .show_ui(ui, |ui| {
                        for z in ZLayerChoice::ALL {
                            ui.selectable_value(&mut s.z_layer, z, z.label());
                        }
                    });
                ui.end_row();

                ui.label("Z-order");
                ui.add(egui::DragValue::new(&mut s.z_order).range(-100..=100));
                ui.end_row();

                ui.label("Warstwa edytora");
                ui.add(egui::DragValue::new(&mut s.editor_layer).range(0..=1000));
                ui.end_row();

                ui.label("Zakładka (szpary)");
                ui.add(
                    egui::DragValue::new(&mut s.overlap)
                        .range(0.0..=2.0)
                        .speed(0.01)
                        .suffix(" j."),
                );
                ui.end_row();
            });
            ui.checkbox(
                &mut self.settings.no_touch,
                "No Touch (bez kolizji z graczem)",
            );
            ui.checkbox(
                &mut self.settings.no_enter_effects,
                "Don't Fade + Don't Enter",
            );

            if let Some(p) = &self.processed {
                let check = self
                    .settings
                    .validate(p.palette.colors.len(), p.frames.len());
                if let Err(e) = &check {
                    ui.colored_label(Color32::LIGHT_RED, e.to_string());
                }
                ui.add_space(6.0);
                if ui
                    .add_enabled(check.is_ok(), egui::Button::new("Eksportuj .gmd…"))
                    .clicked()
                {
                    self.export_gmd();
                }
            }

            ui.separator();
            ui.label(&self.status);
        });
    }

    fn preview(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.add(
                egui::Slider::new(&mut self.zoom, 1.0..=64.0)
                    .logarithmic(true)
                    .text("zoom"),
            );
            ui.checkbox(&mut self.show_grid, "siatka");
            ui.checkbox(&mut self.show_rects, "obiekty (prostokąty)");
            ui.checkbox(&mut self.show_original, "oryginał");
        });
        ui.label("Ctrl + kółko myszy = zoom. Plik PNG można też przeciągnąć na okno.");
        ui.separator();

        // egui zamienia Ctrl + kółko (i gest szczypania) na zoom_delta.
        let zoom_delta = ui.input(|i| i.zoom_delta());
        if zoom_delta != 1.0 {
            self.zoom = (self.zoom * zoom_delta).clamp(1.0, 64.0);
        }

        let tex = if self.show_original {
            &self.tex_original
        } else {
            &self.tex_reduced
        };
        let (Some(tex), Some(processed)) = (tex, &self.processed) else {
            return;
        };
        let frame = &processed.frames[0];
        let (w, h) = (frame.indexed.width as f32, frame.indexed.height as f32);
        if self.fit_pending {
            let avail = ui.available_size();
            self.zoom = (avail.x / w).min(avail.y / h).floor().clamp(1.0, 64.0);
            self.fit_pending = false;
        }
        let zoom = self.zoom;

        egui::ScrollArea::both().show(ui, |ui| {
            let (rect, _) = ui.allocate_exact_size(Vec2::new(w, h) * zoom, egui::Sense::hover());
            let painter = ui.painter_at(rect);

            // szachownica pod przezroczystymi pikselami
            let cell = 8.0;
            let mut y = rect.top();
            let mut row = 0;
            while y < rect.bottom() {
                let mut x = rect.left();
                let mut col = row;
                while x < rect.right() {
                    let c = if col % 2 == 0 {
                        Color32::from_gray(60)
                    } else {
                        Color32::from_gray(80)
                    };
                    painter.rect_filled(
                        Rect::from_min_size(Pos2::new(x, y), Vec2::splat(cell)),
                        0.0,
                        c,
                    );
                    x += cell;
                    col += 1;
                }
                y += cell;
                row += 1;
            }

            painter.image(
                tex.id(),
                rect,
                Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
                Color32::WHITE,
            );

            if self.show_grid && zoom >= 4.0 {
                let stroke = Stroke::new(1.0, Color32::from_black_alpha(60));
                for i in 0..=frame.indexed.width {
                    let x = rect.left() + i as f32 * zoom;
                    painter.line_segment(
                        [Pos2::new(x, rect.top()), Pos2::new(x, rect.bottom())],
                        stroke,
                    );
                }
                for j in 0..=frame.indexed.height {
                    let y = rect.top() + j as f32 * zoom;
                    painter.line_segment(
                        [Pos2::new(rect.left(), y), Pos2::new(rect.right(), y)],
                        stroke,
                    );
                }
            }

            if self.show_rects {
                let stroke = Stroke::new(1.0, Color32::from_rgba_unmultiplied(255, 230, 0, 170));
                for r in &frame.rects {
                    let min = rect.min + Vec2::new(r.x as f32, r.y as f32) * zoom;
                    let size = Vec2::new(r.w as f32, r.h as f32) * zoom;
                    painter.rect_stroke(
                        Rect::from_min_size(min, size),
                        0.0,
                        stroke,
                        StrokeKind::Inside,
                    );
                }
            }
        });
    }
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        let dropped = ctx.input(|i| i.raw.dropped_files.first().map(|f| f.path().to_path_buf()));
        if let Some(path) = dropped {
            self.load_path(&ctx, path);
        }
        self.refresh(&ctx);
        egui::Panel::left("settings")
            .resizable(true)
            .default_size(320.0)
            .show(ui, |ui| self.side_panel(ui));
        egui::CentralPanel::default_margins().show(ui, |ui| self.preview(ui));
    }
}
