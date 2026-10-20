//! Visual typesetting on the video (Aegisub's visual tools): drag `\pos` and both ends of a
//! `\move`, `\org`, rotation around Z and X/Y, scale, rectangular and vector clips. Every drag
//! is one undo step; Shift locks the axis (position, X/Y rotation), snaps Z rotation to 15° and
//! scales X and Y together.

use super::*;
use asscore::typeset::{self as ts, Clip, Drawing, Move};

#[derive(PartialEq, Clone, Copy, Debug)]
pub(super) enum Tool { None, Pos, Org, Rotate, RotateXY, Scale, Clip, VClip }

/// What a drag holds, with the values at its start (moves are relative to them).
#[derive(Clone, Debug)]
pub(super) enum VDrag {
    Pos { origin: (f64, f64), offset: (f64, f64) },
    MoveEnd { end: bool, offset: (f64, f64) },
    Org,
    Rotate { org: (f64, f64), start_angle: f64, frz0: f64 },
    RotateXY { start: egui::Pos2, frx0: f64, fry0: f64 },
    Scale { start: egui::Pos2, sx0: f64, sy0: f64 },
    ClipCorner { corner: u8 },
    ClipMove { start: (f64, f64), rect0: (f64, f64, f64, f64) },
    ClipNew { start: (f64, f64) },
    VPoint { k: usize },
}

pub(super) struct Visual {
    pub tool: Tool,
    pub clip_inverse: bool,
    pub drag: Option<VDrag>,
}

impl Default for Visual {
    fn default() -> Self { Visual { tool: Tool::None, clip_inverse: false, drag: None } }
}

const HANDLE: f32 = 9.0;
const YELLOW: Color32 = Color32::from_rgb(255, 210, 60);
const CYAN: Color32 = Color32::from_rgb(120, 220, 255);
const GREEN: Color32 = Color32::from_rgb(120, 255, 160);

fn rect_of(c: &Clip) -> Option<(f64, f64, f64, f64)> {
    match c { Clip::Rect { x1, y1, x2, y2, .. } => Some((x1.min(*x2), y1.min(*y2), x1.max(*x2), y1.max(*y2))), _ => None }
}

impl App {
    pub(super) fn visual_toolbar(&mut self, ui: &mut egui::Ui) {
        ui.horizontal_wrapped(|ui| {
            ui.label(t("Tool:"));
            for (tool, l, hint) in [
                (Tool::None, t("–"), t("No tool")),
                (Tool::Pos, t("Pos"), t("Click or drag to set \\pos; with \\move drag either end")),
                (Tool::Org, t("Org"), t("Set \\org (the rotation origin)")),
                (Tool::Rotate, t("Rot Z"), t("Drag around the origin to rotate (\\frz); Shift snaps to 15°")),
                (Tool::RotateXY, t("Rot XY"), t("Drag to tilt (\\frx, \\fry)")),
                (Tool::Scale, t("Scale"), t("Drag right/up to scale (\\fscx, \\fscy); Shift keeps the ratio")),
                (Tool::Clip, t("Clip"), t("Drag a rectangle (\\clip); drag its corners or inside to adjust")),
                (Tool::VClip, t("VClip"), t("Vector clip: click adds a point, drag moves one, right-click deletes")),
            ] {
                ui.selectable_value(&mut self.vis.tool, tool, l).on_hover_text(hint);
            }
            let i = self.active;
            let Some(text) = self.doc.events.get(i).map(|e| e.text.clone()) else { return };
            match self.vis.tool {
                Tool::Pos => {
                    let has_move = ts::get_move(&text).is_some();
                    if ui.small_button(if has_move { t("move → pos") } else { t("pos → move") }).on_hover_text(t("Switch the line between \\pos and \\move")).clicked() {
                        self.begin_edit(0);
                        let new = match ts::get_move(&text) {
                            Some(m) => ts::set_pos(&text, m.x1, m.y1),
                            None => {
                                let (x, y) = self.line_anchor(i);
                                let d = self.doc.events[i].duration();
                                ts::set_move(&text, &Move { x1: x, y1: y, x2: x, y2: y, times: Some((0, d)) })
                            }
                        };
                        self.doc.events[i].text = new;
                        self.after_visual_edit();
                    }
                }
                Tool::Clip | Tool::VClip => {
                    let mut inv = self.vis.clip_inverse;
                    if ui.checkbox(&mut inv, t("inverse")).changed() {
                        self.vis.clip_inverse = inv;
                        // flip the line's clip too
                        if let Some(c) = ts::get_clip(&text) {
                            self.begin_edit(0);
                            self.doc.events[i].text = match c {
                                Clip::Rect { x1, y1, x2, y2, .. } => ts::set_clip_rect(&text, x1, y1, x2, y2, inv),
                                Clip::Vector { drawing, .. } => ts::set_clip_vector(&text, &drawing, inv),
                            };
                            self.after_visual_edit();
                        }
                    }
                    if ts::get_clip(&text).is_some() && ui.small_button(t("Remove clip")).clicked() {
                        self.begin_edit(0);
                        self.doc.events[i].text = ts::remove_clip(&text);
                        self.after_visual_edit();
                    }
                }
                _ => {}
            }
        });
    }

    /// Where the line is anchored: \pos, the start of \move, or its alignment inside the margins.
    pub(super) fn line_anchor(&self, i: usize) -> (f64, f64) {
        let Some(ev) = self.doc.events.get(i) else { return (0.0, 0.0) };
        let (rx, ry) = self.doc.res();
        let st = self.doc.style_or_default(&ev.style);
        let an = ts::get_tag(&ev.text, "an").and_then(|a| a.trim().parse().ok()).unwrap_or(st.alignment);
        ts::get_pos(&ev.text).unwrap_or_else(|| ts::default_anchor(an, (rx.max(1), ry.max(1)),
            if ev.margin_l > 0 { ev.margin_l } else { st.margin_l }, if ev.margin_r > 0 { ev.margin_r } else { st.margin_r }, if ev.margin_v > 0 { ev.margin_v } else { st.margin_v }))
    }

    fn after_visual_edit(&mut self) {
        self.buf_key = (usize::MAX, 0, 0);
        self.media.last_render_key = 0;
    }

    pub(super) fn visual_overlay(&mut self, ui: &mut egui::Ui, img: &egui::Response) {
        let rect = img.rect;
        let (rx, ry) = self.doc.res();
        let (rx, ry) = (rx.max(1) as f64, ry.max(1) as f64);
        let to_script = |p: egui::Pos2| ((p.x - rect.left()) as f64 / rect.width() as f64 * rx, (p.y - rect.top()) as f64 / rect.height() as f64 * ry);
        let to_screen = |x: f64, y: f64| egui::Pos2::new(rect.left() + (x / rx) as f32 * rect.width(), rect.top() + (y / ry) as f32 * rect.height());
        let i = self.active;
        let Some(ev) = self.doc.events.get(i).cloned() else { return };
        let tool = self.vis.tool;
        if tool == Tool::None { return; }
        let st = self.doc.style_or_default(&ev.style);
        let anchor = self.line_anchor(i);
        let mv = ts::get_move(&ev.text);
        let org = ts::get_org(&ev.text).unwrap_or(anchor);
        let clip = ts::get_clip(&ev.text);
        let painter = ui.painter_at(rect);
        let stroke = |c: Color32| egui::Stroke::new(1.5_f32, c);
        let cross = |p: egui::Pos2, c: Color32| {
            painter.circle_stroke(p, 7.0, stroke(c));
            painter.line_segment([p - egui::vec2(12.0, 0.0), p + egui::vec2(12.0, 0.0)], egui::Stroke::new(1.0_f32, c));
            painter.line_segment([p - egui::vec2(0.0, 12.0), p + egui::vec2(0.0, 12.0)], egui::Stroke::new(1.0_f32, c));
        };
        // ------------------------------------------------ drawing
        let a = to_screen(anchor.0, anchor.1);
        match tool {
            Tool::Pos => match mv {
                Some(m) => {
                    let (p1, p2) = (to_screen(m.x1, m.y1), to_screen(m.x2, m.y2));
                    painter.line_segment([p1, p2], egui::Stroke::new(1.0_f32, YELLOW));
                    painter.rect_stroke(egui::Rect::from_center_size(p1, egui::vec2(12.0, 12.0)), 0.0, stroke(YELLOW), egui::StrokeKind::Middle);
                    painter.circle_stroke(p2, 7.0, stroke(YELLOW));
                    painter.circle_filled(p2, 2.5, YELLOW);
                }
                None => cross(a, YELLOW),
            },
            Tool::Org => {
                cross(a, YELLOW);
                let o = to_screen(org.0, org.1);
                painter.line_segment([a, o], egui::Stroke::new(1.0_f32, CYAN));
                painter.circle_filled(o, 5.0, CYAN);
            }
            Tool::Rotate => {
                let o = to_screen(org.0, org.1);
                let frz = ts::get_num(&ev.text, "frz").or_else(|| ts::get_num(&ev.text, "fr")).unwrap_or(st.angle);
                painter.circle_stroke(o, 60.0, egui::Stroke::new(1.0_f32, CYAN));
                let ang = (frz as f32).to_radians();
                painter.line_segment([o, o + egui::vec2(ang.cos(), -ang.sin()) * 60.0], stroke(YELLOW));
                painter.circle_filled(o, 4.0, CYAN);
                painter.text(o + egui::vec2(66.0, -8.0), egui::Align2::LEFT_CENTER, format!("frz {:.1}°", frz), egui::FontId::proportional(13.0), YELLOW);
            }
            Tool::RotateXY => {
                let o = to_screen(org.0, org.1);
                painter.line_segment([o - egui::vec2(60.0, 0.0), o + egui::vec2(60.0, 0.0)], egui::Stroke::new(1.0_f32, CYAN));
                painter.line_segment([o - egui::vec2(0.0, 60.0), o + egui::vec2(0.0, 60.0)], egui::Stroke::new(1.0_f32, CYAN));
                painter.circle_filled(o, 4.0, CYAN);
                let (frx, fry) = (ts::get_num(&ev.text, "frx").unwrap_or(0.0), ts::get_num(&ev.text, "fry").unwrap_or(0.0));
                painter.text(o + egui::vec2(8.0, -70.0), egui::Align2::LEFT_CENTER, format!("frx {:.1}° · fry {:.1}°", frx, fry), egui::FontId::proportional(13.0), YELLOW);
            }
            Tool::Scale => {
                cross(a, YELLOW);
                let (sx, sy) = (ts::get_num(&ev.text, "fscx").unwrap_or(st.scale_x), ts::get_num(&ev.text, "fscy").unwrap_or(st.scale_y));
                let (hx, hy) = (a + egui::vec2((sx as f32 * 0.6).clamp(10.0, 300.0), 0.0), a - egui::vec2(0.0, (sy as f32 * 0.6).clamp(10.0, 300.0)));
                painter.line_segment([a, hx], egui::Stroke::new(1.0_f32, CYAN));
                painter.line_segment([a, hy], egui::Stroke::new(1.0_f32, CYAN));
                painter.rect_filled(egui::Rect::from_center_size(hx, egui::vec2(8.0, 8.0)), 0.0, CYAN);
                painter.rect_filled(egui::Rect::from_center_size(hy, egui::vec2(8.0, 8.0)), 0.0, CYAN);
                painter.text(a + egui::vec2(10.0, 14.0), egui::Align2::LEFT_CENTER, format!("{:.0}% × {:.0}%", sx, sy), egui::FontId::proportional(13.0), YELLOW);
            }
            Tool::Clip => {
                if let Some((x1, y1, x2, y2)) = clip.as_ref().and_then(rect_of) {
                    let r = egui::Rect::from_two_pos(to_screen(x1, y1), to_screen(x2, y2));
                    painter.rect_stroke(r, 0.0, stroke(GREEN), egui::StrokeKind::Middle);
                    for c in [r.left_top(), r.right_top(), r.right_bottom(), r.left_bottom()] { painter.rect_filled(egui::Rect::from_center_size(c, egui::vec2(7.0, 7.0)), 0.0, GREEN); }
                }
            }
            Tool::VClip => {
                if let Some(Clip::Vector { drawing, .. }) = &clip {
                    let pts: Vec<egui::Pos2> = drawing.points().iter().map(|&(x, y)| to_screen(x, y)).collect();
                    if pts.len() > 1 {
                        let mut closed = pts.clone();
                        closed.push(pts[0]);
                        painter.add(egui::Shape::line(closed, stroke(GREEN)));
                    }
                    for (k, p) in pts.iter().enumerate() { painter.circle_filled(*p, if k == 0 { 5.0 } else { 4.0 }, GREEN); }
                }
            }
            Tool::None => {}
        }
        if ui.rect_contains_pointer(rect) { ui.ctx().set_cursor_icon(egui::CursorIcon::Crosshair); }

        // ------------------------------------------------ interaction
        let shift = ui.input(|i| i.modifiers.shift);
        if img.drag_stopped() {
            self.vis.drag = None;
            self.last_edit_key = 0;
        }
        let near = |p: egui::Pos2, x: f64, y: f64| to_screen(x, y).distance(p) <= HANDLE;
        // clicks without a drag
        if img.clicked() || img.secondary_clicked() {
            let Some(pp) = img.interact_pointer_pos() else { return };
            let (mx, my) = to_script(pp);
            let (mx, my) = (mx.clamp(0.0, rx).round(), my.clamp(0.0, ry).round());
            match tool {
                Tool::Pos if img.clicked() && mv.is_none() => {
                    self.begin_edit(0);
                    self.move_selected_pos(i, &ev.text, anchor, mx, my);
                }
                Tool::Org if img.clicked() => { self.begin_edit(0); self.doc.events[i].text = ts::set_org(&ev.text, mx, my); }
                Tool::VClip => {
                    let mut d = match &clip { Some(Clip::Vector { drawing, .. }) => drawing.clone(), _ => Drawing::default() };
                    let hit = d.points().iter().position(|&(x, y)| near(pp, x, y));
                    if img.secondary_clicked() {
                        let Some(k) = hit else { return };
                        d.remove_point(k);
                    } else if hit.is_none() {
                        d.push_point(mx, my);
                    } else { return; }
                    self.begin_edit(0);
                    self.doc.events[i].text = if d.cmds.is_empty() { ts::remove_clip(&ev.text) } else { ts::set_clip_vector(&ev.text, &d, self.vis.clip_inverse) };
                }
                _ => return,
            }
            self.after_visual_edit();
            return;
        }
        if img.drag_started_by(egui::PointerButton::Primary) {
            let p0 = ui.input(|i| i.pointer.press_origin()).unwrap_or_else(|| img.interact_pointer_pos().unwrap_or(rect.center()));
            let (sx0, sy0) = to_script(p0);
            self.vis.drag = match tool {
                Tool::Pos => match mv {
                    Some(m) if near(p0, m.x2, m.y2) => Some(VDrag::MoveEnd { end: true, offset: (sx0 - m.x2, sy0 - m.y2) }),
                    Some(m) => Some(VDrag::MoveEnd { end: false, offset: if near(p0, m.x1, m.y1) { (sx0 - m.x1, sy0 - m.y1) } else { (0.0, 0.0) } }),
                    // grabbing the anchor moves it by the drag; elsewhere it jumps to the pointer
                    None => Some(VDrag::Pos { origin: anchor, offset: if near(p0, anchor.0, anchor.1) { (sx0 - anchor.0, sy0 - anchor.1) } else { (0.0, 0.0) } }),
                },
                Tool::Org => Some(VDrag::Org),
                Tool::Rotate => {
                    let frz0 = ts::get_num(&ev.text, "frz").or_else(|| ts::get_num(&ev.text, "fr")).unwrap_or(st.angle);
                    Some(VDrag::Rotate { org, start_angle: (-(sy0 - org.1)).atan2(sx0 - org.0).to_degrees(), frz0 })
                }
                Tool::RotateXY => Some(VDrag::RotateXY { start: p0, frx0: ts::get_num(&ev.text, "frx").unwrap_or(0.0), fry0: ts::get_num(&ev.text, "fry").unwrap_or(0.0) }),
                Tool::Scale => Some(VDrag::Scale { start: p0, sx0: ts::get_num(&ev.text, "fscx").unwrap_or(st.scale_x), sy0: ts::get_num(&ev.text, "fscy").unwrap_or(st.scale_y) }),
                Tool::Clip => match clip.as_ref().and_then(rect_of) {
                    Some((x1, y1, x2, y2)) => {
                        let corners = [(x1, y1), (x2, y1), (x2, y2), (x1, y2)];
                        if let Some(c) = corners.iter().position(|&(x, y)| near(p0, x, y)) { Some(VDrag::ClipCorner { corner: c as u8 }) }
                        else if sx0 > x1 && sx0 < x2 && sy0 > y1 && sy0 < y2 { Some(VDrag::ClipMove { start: (sx0, sy0), rect0: (x1, y1, x2, y2) }) }
                        else { Some(VDrag::ClipNew { start: (sx0, sy0) }) }
                    }
                    None => Some(VDrag::ClipNew { start: (sx0, sy0) }),
                },
                Tool::VClip => match &clip {
                    Some(Clip::Vector { drawing, .. }) => drawing.points().iter().position(|&(x, y)| near(p0, x, y)).map(|k| VDrag::VPoint { k }),
                    _ => None,
                },
                Tool::None => None,
            };
        }
        let Some(drag) = self.vis.drag.clone() else { return };
        if !img.dragged_by(egui::PointerButton::Primary) { return; }
        let Some(pp) = img.interact_pointer_pos() else { return };
        let (mx, my) = to_script(pp);
        let (mx, my) = (mx.clamp(0.0, rx), my.clamp(0.0, ry));
        let key = 9_000_000 + (tool as u64) * 100_000 + i as u64;
        let text = ev.text.clone();
        let new = match drag {
            VDrag::Pos { origin, offset } => {
                let (mut x, mut y) = ((mx - offset.0).round(), (my - offset.1).round());
                if shift { if (x - origin.0).abs() > (y - origin.1).abs() { y = origin.1; } else { x = origin.0; } }
                self.begin_edit(key);
                self.move_selected_pos(i, &text, anchor, x, y);
                None
            }
            VDrag::MoveEnd { end, offset } => {
                let Some(mut m) = mv else { return };
                let (x, y) = ((mx - offset.0).round(), (my - offset.1).round());
                if end { m.x2 = x; m.y2 = y; } else { m.x1 = x; m.y1 = y; }
                Some(ts::set_move(&text, &m))
            }
            VDrag::Org => Some(ts::set_org(&text, mx.round(), my.round())),
            VDrag::Rotate { org, start_angle, frz0 } => {
                let now = (-(my - org.1)).atan2(mx - org.0).to_degrees();
                let mut frz = frz0 + now - start_angle;
                while frz > 360.0 { frz -= 360.0; }
                while frz < -360.0 { frz += 360.0; }
                if shift { frz = (frz / 15.0).round() * 15.0; }
                Some(ts::set_frz(&text, frz))
            }
            VDrag::RotateXY { start, frx0, fry0 } => {
                let (mut dx, mut dy) = ((pp.x - start.x) as f64, (pp.y - start.y) as f64);
                if shift { if dx.abs() > dy.abs() { dy = 0.0; } else { dx = 0.0; } }
                let t1 = ts::set_num(&text, "fry", fry0 + dx * 0.5, &[]);
                Some(ts::set_num(&t1, "frx", frx0 - dy * 0.5, &[]))
            }
            VDrag::Scale { start, sx0, sy0 } => {
                let (dx, dy) = ((pp.x - start.x) as f64, (pp.y - start.y) as f64);
                let (sx, sy) = if shift { let f = (1.0 + dx / 200.0).max(0.0); (sx0 * f, sy0 * f) } else { ((sx0 + dx * 0.5).max(0.0), (sy0 - dy * 0.5).max(0.0)) };
                let t1 = ts::set_num(&text, "fscx", sx, &[]);
                Some(ts::set_num(&t1, "fscy", sy, &[]))
            }
            VDrag::ClipCorner { corner } => {
                let Some((mut x1, mut y1, mut x2, mut y2)) = clip.as_ref().and_then(rect_of) else { return };
                let inv = matches!(clip, Some(Clip::Rect { inverse: true, .. }));
                let (x, y) = (mx.round(), my.round());
                match corner { 0 => { x1 = x; y1 = y; } 1 => { x2 = x; y1 = y; } 2 => { x2 = x; y2 = y; } _ => { x1 = x; y2 = y; } }
                // keep holding the same corner when it crosses the opposite edge
                let c = (if x1 > x2 { 1 } else { 0 }) ^ (if y1 > y2 { 2 } else { 0 });
                if c != 0 { let flip = [0u8, 1, 3, 2]; self.vis.drag = Some(VDrag::ClipCorner { corner: corner ^ flip[c as usize] }); }
                Some(ts::set_clip_rect(&text, x1, y1, x2, y2, inv))
            }
            VDrag::ClipMove { start, rect0 } => {
                let inv = matches!(clip, Some(Clip::Rect { inverse: true, .. }));
                let (dx, dy) = ((mx - start.0).round(), (my - start.1).round());
                Some(ts::set_clip_rect(&text, rect0.0 + dx, rect0.1 + dy, rect0.2 + dx, rect0.3 + dy, inv))
            }
            VDrag::ClipNew { start } => Some(ts::set_clip_rect(&text, start.0.round(), start.1.round(), mx.round(), my.round(), self.vis.clip_inverse)),
            VDrag::VPoint { k } => {
                let Some(Clip::Vector { mut drawing, inverse }) = clip else { return };
                if let Some(p) = drawing.point_mut(k) { *p = (mx.round(), my.round()); }
                Some(ts::set_clip_vector(&text, &drawing, inverse))
            }
        };
        if let Some(t) = new {
            if t != text { self.begin_edit(key); self.doc.events[i].text = t; }
        }
        self.after_visual_edit();
    }

    /// Put the active line at (x, y); other selected lines with \pos move by the same amount.
    fn move_selected_pos(&mut self, i: usize, text: &str, anchor: (f64, f64), x: f64, y: f64) {
        let (dx, dy) = (x - anchor.0, y - anchor.1);
        for k in self.sel_vec() {
            if k == i { self.doc.events[k].text = ts::set_pos(text, x, y); }
            else if let Some(p) = ts::get_pos(&self.doc.events[k].text) { let t = self.doc.events[k].text.clone(); self.doc.events[k].text = ts::set_pos(&t, p.0 + dx, p.1 + dy); }
        }
        if !self.selection.contains(&i) { self.doc.events[i].text = ts::set_pos(text, x, y); }
    }
}
