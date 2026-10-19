use crate::state::App;
use crate::theme;
use eframe::egui::{self, text::LayoutJob, Color32, FontId, RichText, TextFormat};
use subforge_core::tags;
use subforge_core::time::{format_ass, parse_time, Fps};

/// Syntax-highlight `{\tags}` inside the editor.
fn highlight(ui: &egui::Ui, text: &str, wrap: f32, size: f32) -> std::sync::Arc<egui::Galley> {
    let mut job = LayoutJob::default();
    job.wrap.max_width = wrap;
    let normal = TextFormat { font_id: FontId::proportional(size), color: ui.visuals().text_color(), ..Default::default() };
    let tag = TextFormat { font_id: FontId::monospace(size - 1.0), color: theme::ACCENT, ..Default::default() };
    let brk = TextFormat { font_id: FontId::monospace(size - 1.0), color: Color32::from_rgb(120, 170, 255), ..Default::default() };
    let mut rest = text;
    while !rest.is_empty() {
        if let Some(o) = rest.find('{') {
            push_text(&mut job, &rest[..o], &normal, &brk);
            match rest[o..].find('}') {
                Some(c) => {
                    job.append(&rest[o..o + c + 1], 0.0, tag.clone());
                    rest = &rest[o + c + 1..];
                }
                None => {
                    job.append(&rest[o..], 0.0, TextFormat { color: theme::ERR, ..tag.clone() });
                    rest = "";
                }
            }
        } else {
            push_text(&mut job, rest, &normal, &brk);
            rest = "";
        }
    }
    ui.fonts(|f| f.layout_job(job))
}

fn push_text(job: &mut LayoutJob, s: &str, normal: &TextFormat, brk: &TextFormat) {
    let mut last = 0;
    for (i, m) in s.match_indices("\\N").chain(s.match_indices("\\n")).chain(s.match_indices("\\h")) {
        if i >= last {
            job.append(&s[last..i], 0.0, normal.clone());
            job.append(m, 0.0, brk.clone());
            last = i + m.len();
        }
    }
    job.append(&s[last..], 0.0, normal.clone());
}

impl App {
    pub fn edit_panel(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::bottom("editor").resizable(true).default_height(170.0).min_height(120.0).show(ctx, |ui| {
            let Some(idx) = self.active.filter(|&i| i < self.proj.script.events.len()) else {
                ui.centered_and_justified(|ui| {
                    ui.label("Düzenlenecek satır yok. Ctrl+I ile ekleyin.");
                });
                return;
            };
            // refresh time buffers when the line or the data changed from outside
            if self.buf_for != Some((idx, self.version)) {
                let e = &self.proj.script.events[idx];
                self.start_buf = format_ass(e.start);
                self.end_buf = format_ass(e.end);
                self.dur_buf = format!("{:.2}", (e.end - e.start) as f64 / 1000.0);
                self.buf_for = Some((idx, self.version));
            }
            let fps = Fps::from_f64(self.fps());
            let style_names: Vec<String> = self.proj.script.styles.iter().map(|s| s.name.clone()).collect();

            let mut new_start: Option<i64> = None;
            let mut new_end: Option<i64> = None;
            let mut new_dur: Option<i64> = None;
            let mut set_style: Option<String> = None;
            let mut set_actor: Option<String> = None;
            let mut set_layer: Option<i32> = None;
            let mut set_effect: Option<String> = None;
            let mut set_margins: Option<(i32, i32, i32)> = None;
            let mut toggle_comment = false;

            ui.add_space(4.0);
            ui.horizontal_wrapped(|ui| {
                let e = &self.proj.script.events[idx];
                ui.label(RichText::new(format!("Satır {}", idx + 1)).strong().color(theme::ACCENT));
                let mut c = e.is_comment();
                if ui.checkbox(&mut c, "Yorum").changed() {
                    toggle_comment = true;
                }
                ui.separator();
                ui.label("Stil");
                let mut st = e.style.clone();
                egui::ComboBox::from_id_salt("style_combo").selected_text(&st).width(110.0).show_ui(ui, |ui| {
                    for n in &style_names {
                        ui.selectable_value(&mut st, n.clone(), n);
                    }
                });
                if st != e.style {
                    set_style = Some(st);
                }
                ui.label("Konuşmacı");
                let mut actor = e.actor.clone();
                if ui.add(egui::TextEdit::singleline(&mut actor).desired_width(90.0)).changed() {
                    set_actor = Some(actor);
                }
                ui.label("Katman");
                let mut layer = e.layer;
                if ui.add(egui::DragValue::new(&mut layer).range(0..=999)).changed() {
                    set_layer = Some(layer);
                }
                ui.label("Kenar");
                let (mut l, mut r, mut v) = (e.margin_l, e.margin_r, e.margin_b);
                let mut mchanged = false;
                mchanged |= ui.add(egui::DragValue::new(&mut l).range(0..=9999)).changed();
                mchanged |= ui.add(egui::DragValue::new(&mut r).range(0..=9999)).changed();
                mchanged |= ui.add(egui::DragValue::new(&mut v).range(0..=9999)).changed();
                if mchanged {
                    set_margins = Some((l, r, v));
                }
                ui.label("Efekt");
                let mut fx = e.effect.clone();
                if ui.add(egui::TextEdit::singleline(&mut fx).desired_width(80.0)).changed() {
                    set_effect = Some(fx);
                }
            });
            ui.horizontal(|ui| {
                let time_field = |ui: &mut egui::Ui, buf: &mut String, w: f32| -> bool {
                    let r = ui.add(egui::TextEdit::singleline(buf).desired_width(w).font(FontId::monospace(13.0)));
                    r.lost_focus()
                };
                ui.label("Başlangıç");
                if time_field(ui, &mut self.start_buf, 84.0) {
                    new_start = parse_time(&self.start_buf);
                }
                ui.label("Bitiş");
                if time_field(ui, &mut self.end_buf, 84.0) {
                    new_end = parse_time(&self.end_buf);
                }
                ui.label("Süre");
                if time_field(ui, &mut self.dur_buf, 56.0) {
                    new_dur = self.dur_buf.trim().parse::<f64>().ok().map(|d| (d * 1000.0).round() as i64);
                }
                let e = &self.proj.script.events[idx];
                ui.separator();
                for (label, d, which) in [("−1k", -1i64, 0), ("+1k", 1, 0), ("−1k", -1, 1), ("+1k", 1, 1)] {
                    let hint = if which == 0 { "başlangıç" } else { "bitiş" };
                    if ui.small_button(label).on_hover_text(format!("{hint} ± 1 kare")).clicked() {
                        if which == 0 {
                            new_start = Some(fps.start_ms(fps.frame_at(e.start) + d));
                        } else {
                            new_end = Some(fps.start_ms(fps.frame_at(e.end) + d));
                        }
                    }
                    if label == "+1k" && which == 0 {
                        ui.label("|");
                    }
                }
                ui.separator();
                let dur = e.end - e.start;
                let chars = tags::visible_len(&e.text);
                let cps = if dur > 0 { chars as f64 / (dur as f64 / 1000.0) } else { 0.0 };
                let lines = tags::lines(&e.text);
                let longest = lines.iter().map(|l| l.chars().count()).max().unwrap_or(0);
                let cps_col = if cps > self.settings.qc.max_cps { theme::WARN } else { Color32::GRAY };
                ui.label(RichText::new(format!("{chars} kar · {cps:.1} kar/sn")).color(cps_col));
                let lc = if longest > self.settings.qc.max_line_chars { theme::WARN } else { Color32::GRAY };
                ui.label(RichText::new(format!("{} satır · en uzun {longest}", lines.len())).color(lc));
            });

            // text box
            let mut text = self.proj.script.events[idx].text.clone();
            let size = self.settings.grid_font + 3.0;
            let mut layouter = |ui: &egui::Ui, s: &str, wrap: f32| highlight(ui, s, wrap, size);
            let id = egui::Id::new("text_editor");
            let resp = ui.add_sized(
                [ui.available_width(), (ui.available_height() - 4.0).max(40.0)],
                egui::TextEdit::multiline(&mut text).id(id).layouter(&mut layouter).hint_text("Altyazı metni…  (\\N = satır sonu, {\\i1} = etiket)"),
            );
            if self.text_focus {
                ctx.memory_mut(|m| m.request_focus(id));
                self.text_focus = false;
            }
            if resp.changed() {
                // Enter in the box = line break inside the subtitle
                let fixed = text.replace("\r\n", "\\N").replace('\n', "\\N");
                self.proj.edit_coalesced(&format!("text:{idx}"), |s| s.events[idx].text = fixed);
                self.version += 1;
                self.buf_for = Some((idx, self.version));
            }

            // ---- apply collected changes
            let sel = self.selected_vec();
            let targets: Vec<usize> = if sel.contains(&idx) { sel } else { vec![idx] };
            if let Some(s) = set_style {
                self.proj.edit("Set style", |sc| targets.iter().for_each(|&i| sc.events[i].style = s.clone()));
                self.bump();
            }
            if let Some(a) = set_actor {
                self.proj.edit_coalesced("actor", |sc| sc.events[idx].actor = a);
                self.bump();
            }
            if let Some(l) = set_layer {
                self.proj.edit_coalesced("layer", |sc| targets.iter().for_each(|&i| sc.events[i].layer = l));
                self.bump();
            }
            if let Some(f) = set_effect {
                self.proj.edit_coalesced("effect", |sc| sc.events[idx].effect = f);
                self.bump();
            }
            if let Some((l, r, v)) = set_margins {
                self.proj.edit_coalesced("margins", |sc| {
                    for &i in &targets {
                        sc.events[i].margin_l = l;
                        sc.events[i].margin_r = r;
                        sc.events[i].margin_t = v;
                        sc.events[i].margin_b = v;
                    }
                });
                self.bump();
            }
            if toggle_comment {
                self.proj.toggle_comment(&targets);
                self.bump();
            }
            if new_start.is_some() || new_end.is_some() || new_dur.is_some() {
                self.proj.edit("Set times", |sc| {
                    let e = &mut sc.events[idx];
                    if let Some(s) = new_start {
                        e.start = s.max(0);
                    }
                    if let Some(en) = new_end {
                        e.end = en.max(0);
                    }
                    if let Some(d) = new_dur {
                        e.end = e.start + d.max(0);
                    }
                });
                self.bump();
            }
        });
    }
}
