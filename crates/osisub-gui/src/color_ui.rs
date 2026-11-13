//! Ortak renk düğmesi: egui renk seçici + videodan pipet + onaltılık/RGB girişi + son renkler.
//!
//! Pipet: "Pipet" düğmesi bekleme kipini açar; videoda tıklanan noktanın rengi (altyazısız kare,
//! 3×3 ortalama) medya katmanınca `deliver` ile geri verilir ve bekleyen düğme bir sonraki karede
//! değeri alır. Esc iptal eder.
use eframe::egui::{self, Color32, Id, RichText};
use osisub_core::color::Color;

#[derive(Clone, Default)]
struct PickState {
    /// Pipetle renk bekleyen düğme
    waiting: Option<Id>,
    /// Teslim edilen renk (düğme kimliği, rgb)
    result: Option<(Id, [u8; 3])>,
    /// Son kullanılan renkler (en yeni başta)
    recent: Vec<[u8; 3]>,
    /// Son eklemenin düğmesi ve zamanı: sürüklerken her kareyi ayrı renk saymamak için
    last_push: Option<(Id, f64)>,
}

fn key() -> Id {
    Id::unique("osisub_color_pick")
}

fn with<R>(ctx: &egui::Context, f: impl FnOnce(&mut PickState) -> R) -> R {
    ctx.data_mut(|d| f(d.get_temp_mut_or_default::<PickState>(key())))
}

/// Pipet bir video tıklaması bekliyor mu?
pub fn eyedropper_waiting(ctx: &egui::Context) -> bool {
    with(ctx, |s| s.waiting.is_some())
}

/// Videodan alınan rengi bekleyen düğmeye teslim et.
pub fn deliver(ctx: &egui::Context, rgb: [u8; 3]) {
    with(ctx, |s| {
        if let Some(id) = s.waiting.take() {
            s.result = Some((id, rgb));
        }
    });
    ctx.request_repaint();
}

pub fn cancel(ctx: &egui::Context) {
    with(ctx, |s| s.waiting = None);
}

fn push_recent(ctx: &egui::Context, id: Id, rgb: [u8; 3]) {
    let now = ctx.input(|i| i.time);
    with(ctx, |s| {
        // Aynı düğmede 1.5 sn içinde art arda değişim (sürükleme): son girdiyi güncelle
        if let Some((lid, t)) = s.last_push {
            if lid == id && now - t < 1.5 && !s.recent.is_empty() {
                s.recent.remove(0);
            }
        }
        s.recent.retain(|c| *c != rgb);
        s.recent.insert(0, rgb);
        s.recent.truncate(16);
        s.last_push = Some((id, now));
    });
}

/// Renk düğmesi (alfa yok). Değer değiştiyse `true`.
pub fn button(ui: &mut egui::Ui, salt: impl std::hash::Hash + std::fmt::Debug, rgb: &mut [u8; 3]) -> bool {
    let id = Id::unique(("color_btn", salt));
    let ctx = ui.ctx().clone();
    let mut changed = false;
    if let Some(c) = with(&ctx, |s| if s.result.map_or(false, |r| r.0 == id) { s.result.take().map(|r| r.1) } else { None }) {
        *rgb = c;
        changed = true;
        push_recent(&ctx, id, c);
    }
    let r = ui.color_edit_button_srgb(rgb);
    if r.changed() {
        changed = true;
        push_recent(&ctx, id, *rgb);
    }
    let waiting = with(&ctx, |s| s.waiting == Some(id));
    let pick = ui
        .add(egui::Button::new(RichText::new(if waiting { "Videoya tıkla" } else { "Pipet" }).small()).selected(waiting))
        .on_hover_text("Videodan renk seç (altyazısız kare). Esc: iptal\nRenk kutusuna sağ tık: #RRGGBB / ASS girişi, son renkler");
    if pick.clicked() {
        with(&ctx, |s| s.waiting = if waiting { None } else { Some(id) });
    }
    r.context_menu(|ui| {
        let c = Color { r: rgb[0], g: rgb[1], b: rgb[2], a: 0 };
        let buf_id = id.with("hexbuf");
        let mut buf: String = ui.data_mut(|d| d.get_temp(buf_id)).unwrap_or_else(|| c.to_html());
        ui.horizontal(|ui| {
            ui.label("Renk");
            let te = ui.add(egui::TextEdit::singleline(&mut buf).desired_width(140.0).font(egui::TextStyle::Monospace));
            let enter = te.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
            if ui.button("Uygula").clicked() || enter {
                match Color::parse_any(&buf) {
                    Some(n) => {
                        *rgb = [n.r, n.g, n.b];
                        changed = true;
                        push_recent(&ctx, id, *rgb);
                    }
                    None => {
                        ui.label(RichText::new("geçersiz").color(Color32::from_rgb(230, 90, 90)));
                    }
                }
            }
        });
        ui.data_mut(|d| d.insert_temp(buf_id, buf));
        ui.label(RichText::new(format!("ASS {}   HTML {}", c.to_tag_string(), c.to_html())).monospace().weak());
        ui.horizontal(|ui| {
            if ui.button("ASS kopyala").clicked() {
                ui.ctx().copy_text(c.to_tag_string());
                ui.close();
            }
            if ui.button("HTML kopyala").clicked() {
                ui.ctx().copy_text(c.to_html());
                ui.close();
            }
        });
        let recent = with(&ctx, |s| s.recent.clone());
        if !recent.is_empty() {
            ui.separator();
            ui.label("Son renkler");
            ui.horizontal_wrapped(|ui| {
                for c in recent {
                    let b = egui::Button::new("").fill(Color32::from_rgb(c[0], c[1], c[2])).min_size(egui::vec2(20.0, 16.0));
                    if ui.add(b).on_hover_text(Color { r: c[0], g: c[1], b: c[2], a: 0 }.to_tag_string()).clicked() {
                        *rgb = c;
                        changed = true;
                        push_recent(&ctx, id, c);
                        ui.close();
                    }
                }
            });
        }
    });
    changed
}

/// Renk örneği: (x, y) çevresindeki 3×3 pikselin RGB ortalaması (RGBA tampon).
pub fn sample(rgba: &[u8], w: usize, h: usize, x: usize, y: usize) -> [u8; 3] {
    let (mut acc, mut n) = ([0u32; 3], 0u32);
    for yy in y.saturating_sub(1)..=(y + 1).min(h.saturating_sub(1)) {
        for xx in x.saturating_sub(1)..=(x + 1).min(w.saturating_sub(1)) {
            let i = (yy * w + xx) * 4;
            if i + 2 < rgba.len() {
                for k in 0..3 {
                    acc[k] += rgba[i + k] as u32;
                }
                n += 1;
            }
        }
    }
    if n == 0 {
        return [0; 3];
    }
    [(acc[0] / n) as u8, (acc[1] / n) as u8, (acc[2] / n) as u8]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sample_averages_and_clamps() {
        // 2×2: kırmızı, yeşil / mavi, beyaz
        let px = [255, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255, 255, 255, 255, 255];
        assert_eq!(sample(&px, 2, 2, 0, 0), [127, 127, 127]);
        assert_eq!(sample(&px, 1, 1, 0, 0), [255, 0, 0]);
        assert_eq!(sample(&[], 0, 0, 5, 5), [0, 0, 0]);
    }
}
