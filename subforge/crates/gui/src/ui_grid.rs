use crate::state::App;
use crate::theme;
use eframe::egui::{self, Align, Color32, RichText};
use egui_extras::{Column, TableBuilder};
use subforge_core::qc::Severity;
use subforge_core::tags;
use subforge_core::time::format_ass;

impl App {
    pub fn grid(&mut self, ui: &mut egui::Ui) {
        let n = self.proj.script.events.len();
        let row_h = self.settings.grid_font + 8.0;
        let mut clicked: Option<(usize, egui::Modifiers)> = None;
        let mut table = TableBuilder::new(ui)
            .striped(true)
            .resizable(true)
            .sense(egui::Sense::click())
            .cell_layout(egui::Layout::left_to_right(Align::Center))
            .column(Column::exact(46.0))
            .column(Column::exact(38.0))
            .column(Column::exact(86.0))
            .column(Column::exact(86.0))
            .column(Column::exact(56.0))
            .column(Column::exact(50.0))
            .column(Column::initial(100.0).at_least(60.0))
            .column(Column::initial(80.0).at_least(40.0));
        let has_ref = self.reference.is_some();
        if has_ref {
            table = table.column(Column::initial(300.0).at_least(80.0).clip(true));
        }
        let mut table = table.column(Column::remainder().at_least(200.0).clip(true));
        if let Some(i) = self.scroll_to.take() {
            table = table.scroll_to_row(i, Some(Align::Center));
        }
        let fps = self.fps();
        let _ = fps;
        table
            .header(22.0, |mut h| {
                for t in ["#", "K", "Başlangıç", "Bitiş", "Süre", "CPS", "Stil", "Konuşmacı"] {
                    h.col(|ui| {
                        ui.label(RichText::new(t).strong());
                    });
                }
                if has_ref {
                    h.col(|ui| {
                        ui.label(RichText::new("Referans").strong());
                    });
                }
                h.col(|ui| {
                    ui.label(RichText::new("Metin").strong());
                });
            })
            .body(|body| {
                body.rows(row_h, n, |mut row| {
                    let i = row.index();
                    let e = &self.proj.script.events[i];
                    let selected = self.sel.contains(&i);
                    row.set_selected(selected);
                    let comment = e.is_comment();
                    let sev = self.issue_sev.get(&i).copied();
                    let dur = e.end - e.start;
                    let cps = if dur > 0 { tags::visible_len(&e.text) as f64 / (dur as f64 / 1000.0) } else { 0.0 };
                    let base = if comment { Color32::from_gray(130) } else { Color32::PLACEHOLDER };
                    let txt = |s: String| RichText::new(s).monospace().size(self.settings.grid_font - 1.0);
                    row.col(|ui| {
                        let mut t = txt(format!("{}", i + 1));
                        if Some(i) == self.active {
                            t = t.color(Color32::WHITE).strong();
                        }
                        ui.label(t);
                    });
                    row.col(|ui| {
                        let (c, s) = match sev {
                            Some(Severity::Error) => (theme::ERR, "!"),
                            Some(Severity::Warning) => (theme::WARN, "!"),
                            Some(Severity::Info) => (Color32::from_gray(140), "i"),
                            None => (theme::OK, ""),
                        };
                        ui.label(RichText::new(s).color(c));
                        if comment {
                            ui.label(RichText::new("#").color(Color32::from_gray(150)));
                        }
                    });
                    row.col(|ui| {
                        ui.label(txt(format_ass(e.start)).color(base));
                    });
                    row.col(|ui| {
                        ui.label(txt(format_ass(e.end)).color(base));
                    });
                    row.col(|ui| {
                        ui.label(txt(format!("{:.2}", dur as f64 / 1000.0)).color(if dur < 0 { theme::ERR } else { base }));
                    });
                    row.col(|ui| {
                        let c = if cps > self.settings.qc.max_cps { theme::WARN } else { base };
                        ui.label(txt(format!("{cps:.1}")).color(c));
                    });
                    row.col(|ui| {
                        ui.label(RichText::new(&e.style).color(base));
                    });
                    row.col(|ui| {
                        ui.label(RichText::new(&e.actor).color(base));
                    });
                    if has_ref {
                        row.col(|ui| {
                            let r = self.ref_texts.get(i).map(|s| s.as_str()).unwrap_or("");
                            ui.add(egui::Label::new(RichText::new(r).size(self.settings.grid_font).color(Color32::from_rgb(140, 170, 210))).truncate());
                        });
                    }
                    row.col(|ui| {
                        let shown = e.text.replace("\\N", " | ");
                        ui.add(egui::Label::new(RichText::new(shown).size(self.settings.grid_font).color(base)).truncate());
                    });
                    let resp = row.response();
                    if resp.clicked() {
                        clicked = Some((i, resp.ctx.input(|i| i.modifiers)));
                    }
                    resp.context_menu(|ui| {
                        if ui.button("Ekle (sonrasına)").clicked() {
                            clicked = Some((i, egui::Modifiers::NONE));
                            ui.close_menu();
                        }
                    });
                });
            });
        if let Some((i, m)) = clicked {
            if m.shift {
                let (a, b) = (self.anchor.min(i), self.anchor.max(i));
                self.sel = (a..=b).collect();
                self.active = Some(i);
            } else if m.command {
                if !self.sel.remove(&i) {
                    self.sel.insert(i);
                }
                self.active = Some(i);
                self.anchor = i;
            } else {
                self.goto_line(i);
                self.scroll_to = None;
            }
            if let (Some(v), Some(e)) = (&mut self.video, self.proj.script.events.get(i)) {
                v.want_ms = e.start;
            }
        }
        if n == 0 {
            ui_empty(self);
        }
    }
}

fn ui_empty(_app: &mut App) {}
