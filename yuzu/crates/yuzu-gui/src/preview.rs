//! Lightweight subtitle preview: renders the active line's styled text over a PlayRes-shaped canvas.

use asscore::{tags, Color, Event, Script, Style};
use eframe::egui::{self, Align2, Color32, FontId, Pos2, Rect, Stroke, Vec2};

fn c32(c: &Color) -> Color32 {
    Color32::from_rgba_unmultiplied(c.r, c.g, c.b, 255 - c.a)
}

#[derive(Default, Clone, Copy)]
struct Overrides {
    an: Option<i32>,
    pos: Option<(f32, f32)>,
}

fn overrides(text: &str) -> Overrides {
    let mut o = Overrides::default();
    for p in tags::split_parts(text) {
        if let tags::Part::Block(b) = p {
            for t in tags::split_block_tags(&b) {
                let (n, a) = tags::tag_name_arg(&t);
                match n.as_str() {
                    "an" => o.an = a.trim().parse().ok(),
                    "a" => {
                        if let Ok(v) = a.trim().parse::<i32>() {
                            // legacy SSA alignment
                            o.an = Some(match v { 1..=3 => v, 5..=7 => v - 5 + 7, 9..=11 => v - 9 + 4, _ => 2 });
                        }
                    }
                    "pos" => {
                        let inner = a.trim_start_matches('(').trim_end_matches(')');
                        let v: Vec<f32> = inner.split(',').filter_map(|x| x.trim().parse().ok()).collect();
                        if v.len() >= 2 { o.pos = Some((v[0], v[1])); }
                    }
                    _ => {}
                }
            }
        }
    }
    o
}

pub fn show(ui: &mut egui::Ui, sc: &Script, ev: Option<&Event>, size: Vec2) {
    let (rx, ry) = sc.res();
    let (rx, ry) = (rx.max(1) as f32, ry.max(1) as f32);
    let aspect = rx / ry;
    let mut w = size.x;
    let mut h = w / aspect;
    if h > size.y { h = size.y; w = h * aspect; }
    let (rect, _) = ui.allocate_exact_size(Vec2::new(w, h), egui::Sense::hover());
    let p = ui.painter_at(rect);
    p.rect_filled(rect, 2.0, Color32::from_rgb(24, 26, 32));
    // checker hint of a 16:9 frame
    for i in 1..3 {
        let x = rect.left() + rect.width() * i as f32 / 3.0;
        let y = rect.top() + rect.height() * i as f32 / 3.0;
        p.line_segment([Pos2::new(x, rect.top()), Pos2::new(x, rect.bottom())], Stroke::new(1.0_f32, Color32::from_gray(40)));
        p.line_segment([Pos2::new(rect.left(), y), Pos2::new(rect.right(), y)], Stroke::new(1.0_f32, Color32::from_gray(40)));
    }
    let Some(ev) = ev else { return };
    let st: Style = sc.style_or_default(&ev.style);
    let scale = h / ry;
    let o = overrides(&ev.text);
    let an = o.an.unwrap_or(st.alignment);
    let disp = tags::display_text(&ev.text);
    if disp.trim().is_empty() { return; }
    let font = FontId::proportional((st.fontsize as f32 * scale).max(6.0));
    let ml = if ev.margin_l > 0 { ev.margin_l } else { st.margin_l } as f32 * scale;
    let mr = if ev.margin_r > 0 { ev.margin_r } else { st.margin_r } as f32 * scale;
    let mv = if ev.margin_v > 0 { ev.margin_v } else { st.margin_v } as f32 * scale;
    let (hcol, vrow) = ((an - 1) % 3, (an - 1) / 3); // 0 left,1 center,2 right ; 0 bottom,1 middle,2 top
    let (anchor_x, align_h) = match (o.pos, hcol) {
        (Some((x, _)), 0) => (rect.left() + x * scale, egui::Align::Min),
        (Some((x, _)), 1) => (rect.left() + x * scale, egui::Align::Center),
        (Some((x, _)), _) => (rect.left() + x * scale, egui::Align::Max),
        (None, 0) => (rect.left() + ml, egui::Align::Min),
        (None, 1) => (rect.center().x + (ml - mr) / 2.0, egui::Align::Center),
        (None, _) => (rect.right() - mr, egui::Align::Max),
    };
    let anchor_y = match (o.pos, vrow) {
        (Some((_, y)), _) => rect.top() + y * scale,
        (None, 0) => rect.bottom() - mv,
        (None, 1) => rect.center().y,
        (None, _) => rect.top() + mv,
    };
    let fill = c32(&st.primary);
    let outline = c32(&st.outline_color);
    let shadow = c32(&st.back);
    let ow = (st.outline as f32 * scale).max(0.0);
    let sh = (st.shadow as f32 * scale).max(0.0);
    let lines: Vec<&str> = disp.split('\n').collect();
    let lh = font.size * 1.15;
    let total = lh * lines.len() as f32;
    let top = match vrow { 0 => anchor_y - total, 1 => anchor_y - total / 2.0, _ => anchor_y };
    for (i, line) in lines.iter().enumerate() {
        let y = top + lh * i as f32 + lh / 2.0;
        let (pos, al) = match align_h {
            egui::Align::Min => (Pos2::new(anchor_x, y), Align2::LEFT_CENTER),
            egui::Align::Center => (Pos2::new(anchor_x, y), Align2::CENTER_CENTER),
            egui::Align::Max => (Pos2::new(anchor_x, y), Align2::RIGHT_CENTER),
        };
        if sh > 0.0 && shadow.a() > 0 { p.text(pos + Vec2::splat(sh), al, line, font.clone(), shadow); }
        if ow > 0.0 {
            let n = 12;
            for k in 0..n {
                let a = k as f32 / n as f32 * std::f32::consts::TAU;
                p.text(pos + Vec2::new(a.cos(), a.sin()) * ow, al, line, font.clone(), outline);
            }
        }
        p.text(pos, al, line, font.clone(), fill);
    }
    let _ = Rect::NOTHING;
}
