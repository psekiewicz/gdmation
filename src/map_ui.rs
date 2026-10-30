//! Zakładka „Mapa hitboxów”: podgląd poziomu `.gmd` z podziałem obiektów na kategorie.

use std::path::PathBuf;

use eframe::egui::{self, Color32, Pos2, Rect, Sense, Shape, Stroke, Vec2};
use gdmation::{
    config,
    hitmap::{self, Category, CategoryStore, LoadedLevel},
};

pub struct MapView {
    level: Option<LoadedLevel>,
    used: Vec<(i32, usize)>,
    store: CategoryStore,
    /// Środek widoku w jednostkach GD i powiększenie (piksele na jednostkę).
    camera: (f64, f64),
    zoom: f32,
    fit_pending: bool,
    show_no_collision: bool,
    only_unconfirmed: bool,
    selected: Option<i32>,
    status: String,
}

impl Default for MapView {
    fn default() -> Self {
        Self {
            level: None,
            used: vec![],
            store: config::categories_path()
                .map(|p| CategoryStore::load(&p))
                .unwrap_or_default(),
            camera: (0.0, 0.0),
            zoom: 1.0,
            fit_pending: false,
            show_no_collision: false,
            only_unconfirmed: false,
            selected: None,
            status: "Wczytaj poziom .gmd, aby zobaczyć mapę.".into(),
        }
    }
}

fn color(cat: Category) -> Color32 {
    match cat {
        Category::Block => Color32::from_rgb(90, 130, 210),
        Category::HalfBlock => Color32::from_rgb(60, 200, 220),
        Category::Hazard => Color32::from_rgb(230, 40, 40),
        Category::SlopeShort => Color32::from_rgb(70, 200, 90),
        Category::SlopeLong => Color32::from_rgb(235, 170, 40),
        Category::NoHitbox => Color32::GRAY,
    }
}

fn point_in_polygon(p: Pos2, poly: &[Pos2]) -> bool {
    let mut inside = false;
    let mut j = poly.len() - 1;
    for i in 0..poly.len() {
        let (a, b) = (poly[i], poly[j]);
        if (a.y > p.y) != (b.y > p.y) && p.x < (b.x - a.x) * (p.y - a.y) / (b.y - a.y) + a.x {
            inside = !inside;
        }
        j = i;
    }
    inside
}

impl MapView {
    pub fn open_dialog(&mut self) {
        if let Some(path) = rfd::FileDialog::new()
            .add_filter("Poziom GD", &["gmd"])
            .pick_file()
        {
            self.load(path);
        }
    }

    pub fn load(&mut self, path: PathBuf) {
        match hitmap::load_gmd(&path) {
            Ok(level) => {
                self.used = hitmap::used_ids(&level.objects);
                self.status = format!(
                    "Wczytano „{}”: {} obiektów, {} rodzajów.",
                    level.name,
                    level.objects.len(),
                    self.used.len()
                );
                self.level = Some(level);
                self.fit_pending = true;
                self.selected = None;
            }
            Err(e) => self.status = format!("Błąd: {e:#}"),
        }
    }

    fn save_store(&mut self) {
        if let Some(path) = config::categories_path()
            && let Err(e) = self.store.save(&path)
        {
            self.status = format!("Nie udało się zapisać kategorii: {e:#}");
        }
    }

    pub fn side_panel(&mut self, ui: &mut egui::Ui) {
        ui.heading("Poziom");
        if ui.button("Wczytaj poziom .gmd…").clicked() {
            self.open_dialog();
        }
        ui.label(&self.status);
        let Some(level) = &self.level else {
            return;
        };

        ui.separator();
        ui.heading("Legenda");
        let mut counts = [0usize; 6];
        for o in &level.objects {
            if !o.no_collision {
                let cat = self.store.category(o.id);
                counts[Category::ALL.iter().position(|c| *c == cat).unwrap()] += 1;
            }
        }
        for (cat, n) in Category::ALL.iter().zip(counts) {
            ui.horizontal(|ui| {
                let (r, _) = ui.allocate_exact_size(Vec2::splat(14.0), Sense::hover());
                ui.painter().rect_filled(r, 2.0, color(*cat));
                ui.label(format!("{}: {n}", cat.label()));
            });
        }
        ui.checkbox(
            &mut self.show_no_collision,
            "Pokaż obiekty bez kolizji (No Touch, Passable)",
        );
        ui.label("Mapa pokazuje obrys obiektu w kratce, nie dokładny hitbox gry.");

        ui.separator();
        let confirmed = self
            .used
            .iter()
            .filter(|(id, _)| self.store.confirmed.contains(id))
            .count();
        ui.heading("Użyte obiekty");
        ui.label(format!("Zatwierdzone: {confirmed} / {}", self.used.len()));
        ui.horizontal(|ui| {
            ui.checkbox(&mut self.only_unconfirmed, "tylko niezatwierdzone");
            if ui.button("Zatwierdź wszystkie").clicked() {
                self.store
                    .confirmed
                    .extend(self.used.iter().map(|(id, _)| *id));
                self.save_store();
            }
        });
        ui.label("Kliknij ID, żeby podświetlić obiekty na mapie.");

        let mut changed = false;
        egui::ScrollArea::vertical().show(ui, |ui| {
            egui::Grid::new("used")
                .striped(true)
                .num_columns(6)
                .show(ui, |ui| {
                    for &(id, count) in &self.used {
                        let is_confirmed = self.store.confirmed.contains(&id);
                        if self.only_unconfirmed && is_confirmed {
                            continue;
                        }
                        let name = hitmap::object_name(id).unwrap_or("(nieznany)");
                        let selected = self.selected == Some(id);
                        if ui
                            .selectable_label(selected, format!("{id}"))
                            .on_hover_text(name)
                            .clicked()
                        {
                            self.selected = if selected { None } else { Some(id) };
                        }
                        ui.label(format!("×{count}"));
                        let short: String = name.chars().take(24).collect();
                        ui.weak(if short.len() < name.len() {
                            format!("{short}…")
                        } else {
                            short
                        });

                        let mut cat = self.store.category(id);
                        egui::ComboBox::from_id_salt(("cat", id))
                            .selected_text(cat.label())
                            .width(130.0)
                            .show_ui(ui, |ui| {
                                for c in Category::ALL {
                                    ui.selectable_value(&mut cat, c, c.label());
                                }
                            });
                        if cat != self.store.category(id) {
                            self.store.overrides.insert(id, cat);
                            changed = true;
                        }

                        if matches!(cat, Category::SlopeShort | Category::SlopeLong) {
                            let mut mirror = self.store.mirror(id);
                            if ui
                                .checkbox(&mut mirror, "odbij")
                                .on_hover_text("Odwróć kierunek slope'a")
                                .changed()
                            {
                                self.store.slope_mirror.insert(id, mirror);
                                changed = true;
                            }
                        } else {
                            ui.label("");
                        }

                        let mut ok = is_confirmed;
                        if ui.checkbox(&mut ok, "OK").on_hover_text(name).changed() {
                            if ok {
                                self.store.confirmed.insert(id);
                            } else {
                                self.store.confirmed.remove(&id);
                            }
                            changed = true;
                        }
                        ui.end_row();
                    }
                });
        });
        if changed {
            self.save_store();
        }
    }

    pub fn canvas(&mut self, ui: &mut egui::Ui) {
        let Some(level) = &self.level else {
            ui.centered_and_justified(|ui| {
                ui.label("Brak poziomu. Wczytaj plik .gmd albo przeciągnij go na okno.")
            });
            return;
        };
        ui.label("Przeciągnij myszą = przesuwanie, kółko = zoom. Najedź na obiekt, żeby zobaczyć szczegóły.");

        let (rect, response) = ui.allocate_exact_size(ui.available_size(), Sense::click_and_drag());
        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, 0.0, Color32::from_gray(25));

        let shapes: Vec<(usize, hitmap::Shape)> = level
            .objects
            .iter()
            .enumerate()
            .filter(|(_, o)| self.show_no_collision || !o.no_collision)
            .filter_map(|(i, o)| {
                hitmap::shape(o, self.store.category(o.id), self.store.mirror(o.id)).map(|s| (i, s))
            })
            .collect();

        if self.fit_pending {
            let (mut min, mut max) = ((f64::MAX, f64::MAX), (f64::MIN, f64::MIN));
            for (_, s) in &shapes {
                for &(x, y) in &s.points {
                    min = (min.0.min(x), min.1.min(y));
                    max = (max.0.max(x), max.1.max(y));
                }
            }
            if min.0 <= max.0 {
                // Poziomy są długie i płaskie: dopasuj wysokość (kratka min. 12 px)
                // i pokaż początek poziomu.
                let h = (max.1 - min.1).max(30.0) as f32;
                self.zoom = (rect.height() / h * 0.9).clamp(12.0 / 30.0, 20.0);
                let half_w = (rect.width() / 2.0 / self.zoom) as f64;
                self.camera = (min.0 + half_w * 0.95, (min.1 + max.1) / 2.0);
            }
            self.fit_pending = false;
        }

        if response.dragged() {
            let d = response.drag_delta();
            self.camera.0 -= (d.x / self.zoom) as f64;
            self.camera.1 += (d.y / self.zoom) as f64;
        }
        if response.hovered() {
            let scroll = ui.input(|i| i.smooth_scroll_delta.y);
            if scroll != 0.0 {
                self.zoom = (self.zoom * (1.0 + scroll * 0.002)).clamp(0.01, 20.0);
            }
        }

        let zoom = self.zoom;
        let center = rect.center();
        let cam = self.camera;
        let to_screen = |x: f64, y: f64| {
            Pos2::new(
                center.x + ((x - cam.0) as f32) * zoom,
                center.y - ((y - cam.1) as f32) * zoom,
            )
        };

        // siatka 30 j. i linia y = 0 (dół pierwszego rzędu kratek)
        let cell = 30.0 * zoom;
        if cell >= 6.0 {
            let stroke = Stroke::new(1.0, Color32::from_gray(40));
            let left = cam.0 - (rect.width() / 2.0 / zoom) as f64;
            let bottom = cam.1 - (rect.height() / 2.0 / zoom) as f64;
            let mut gx = (left / 30.0).floor() * 30.0;
            while to_screen(gx, 0.0).x <= rect.right() {
                let x = to_screen(gx, 0.0).x;
                painter.line_segment(
                    [Pos2::new(x, rect.top()), Pos2::new(x, rect.bottom())],
                    stroke,
                );
                gx += 30.0;
            }
            let mut gy = (bottom / 30.0).floor() * 30.0;
            while to_screen(0.0, gy).y >= rect.top() {
                let y = to_screen(0.0, gy).y;
                painter.line_segment(
                    [Pos2::new(rect.left(), y), Pos2::new(rect.right(), y)],
                    stroke,
                );
                gy += 30.0;
            }
        }
        let ground = to_screen(0.0, 0.0).y;
        painter.line_segment(
            [
                Pos2::new(rect.left(), ground),
                Pos2::new(rect.right(), ground),
            ],
            Stroke::new(1.5, Color32::from_gray(110)),
        );

        let pointer = response.hover_pos();
        let mut hovered = None;
        for (i, s) in &shapes {
            let pts: Vec<Pos2> = s.points.iter().map(|&(x, y)| to_screen(x, y)).collect();
            let bb = Rect::from_points(&pts);
            if !bb.intersects(rect) {
                continue;
            }
            let obj = &level.objects[*i];
            let base = color(s.category);
            let (fill, stroke) = if obj.no_collision {
                (
                    Color32::TRANSPARENT,
                    Stroke::new(1.0, Color32::from_gray(120)),
                )
            } else {
                (base.gamma_multiply(0.55), Stroke::new(1.0, base))
            };
            let stroke = if self.selected == Some(obj.id) {
                Stroke::new(2.0, Color32::WHITE)
            } else {
                stroke
            };
            painter.add(Shape::convex_polygon(pts.clone(), fill, stroke));
            if let Some(p) = pointer
                && bb.contains(p)
                && point_in_polygon(p, &pts)
            {
                hovered = Some(*i);
            }
        }

        if let Some(i) = hovered {
            let o = &level.objects[i];
            let text = format!(
                "ID {} – {}\n{}\nX {:.1}, Y {:.1}, obrót {:.0}°, skala {:.2}×{:.2}{}",
                o.id,
                hitmap::object_name(o.id).unwrap_or("(nieznany)"),
                self.store.category(o.id).label(),
                o.x,
                o.y,
                o.rotation,
                o.scale_x,
                o.scale_y,
                if o.no_collision { "\nbez kolizji" } else { "" }
            );
            response.on_hover_text_at_pointer(text);
        }
    }
}
