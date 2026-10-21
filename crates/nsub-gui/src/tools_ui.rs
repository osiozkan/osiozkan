//! Tool windows: script properties/resample, fonts report, select lines, fix common errors, timing post-processor.

use crate::i18n::{self, tr};
use crate::state::App;
use eframe::egui::{self, RichText};
use nsub_core::{fixes, ops, resample};

#[derive(Default)]
pub struct Tools {
    pub show_props: bool,
    pub show_fonts: bool,
    pub show_select: bool,
    pub show_fix: bool,
    pub show_post: bool,
    pub show_stats: bool,
    pub show_kara: bool,
    kara_chars: bool,
    kara_tag: usize,
    kara_msg: String,
    // properties
    res_w: i32,
    res_h: i32,
    res_loaded: bool,
    // fonts
    fonts_report: Vec<(String, bool, Option<String>)>,
    // select
    sel_pattern: String,
    sel_field: usize,
    sel_regex: bool,
    sel_case: bool,
    sel_invert: bool,
    sel_msg: String,
    // fix
    fix_case: usize,
    fix_hi: bool,
    fix_overlaps: bool,
    fix_min_dur: bool,
    fix_spaces: bool,
    fix_cps: bool,
    fix_balance: bool,
    fix_unbreak: bool,
    fix_dups: bool,
    fix_only_selected: bool,
    fix_balance_len: usize,
    fix_msg: String,
    // post-processor
    lead_in: i64,
    lead_out: i64,
    post_msg: String,
}

fn t_(en: &'static str, tr_s: &'static str) -> &'static str {
    if i18n::lang() == i18n::Lang::Tr {
        tr_s
    } else {
        en
    }
}

impl Tools {
    pub fn new() -> Tools {
        Tools { fix_balance_len: 42, lead_in: 120, lead_out: 200, ..Default::default() }
    }

    pub fn windows(&mut self, ctx: &egui::Context, app: &mut App, fonts: &nsub_fonts::FontMetrics) {
        self.props(ctx, app);
        self.fonts(ctx, app, fonts);
        self.select(ctx, app);
        self.fix(ctx, app);
        self.post(ctx, app);
        self.stats(ctx, app);
        self.kara(ctx, app);
    }

    fn stats(&mut self, ctx: &egui::Context, app: &mut App) {
        let mut open = self.show_stats;
        if open {
            let st = nsub_core::qc::stats(&app.doc.script);
            egui::Window::new(t_("Statistics", "İstatistikler")).open(&mut open).collapsible(false).show(ctx, |ui| {
                egui::Grid::new("stats").num_columns(2).show(ui, |ui| {
                    for (k, v) in [
                        (t_("Lines", "Satır"), format!("{} (+{} {})", st.lines, st.comments, t_("comments", "yorum"))),
                        (t_("Words / characters", "Sözcük / karakter"), format!("{} / {}", st.words, st.chars)),
                        (t_("Total on screen", "Ekranda toplam"), format!("{:.1} s", st.total_ms as f64 / 1000.0)),
                        ("CPS", format!("{:.1} avg / {:.1} max", st.avg_cps, st.max_cps)),
                        (t_("Line length", "Satır uzunluğu"), format!("{:.1} avg / {} max", st.avg_line_len, st.longest_line)),
                    ] {
                        ui.label(k);
                        ui.label(v);
                        ui.end_row();
                    }
                });
            });
        }
        self.show_stats = open;
    }

    fn kara(&mut self, ctx: &egui::Context, app: &mut App) {
        let mut open = self.show_kara;
        egui::Window::new(t_("Karaoke tools", "Karaoke araçları")).open(&mut open).collapsible(false).show(ctx, |ui| {
            ui.label(t_("Applies to the selected lines.", "Seçili satırlara uygulanır."));
            ui.checkbox(&mut self.kara_chars, t_("Split per character (otherwise per word)", "Karakter başına böl (aksi halde sözcük başına)"));
            egui::ComboBox::from_id_salt("karatag").selected_text(["\\k", "\\kf", "\\ko"][self.kara_tag]).show_ui(ui, |ui| {
                for (i, n) in ["\\k", "\\kf", "\\ko"].into_iter().enumerate() {
                    ui.selectable_value(&mut self.kara_tag, i, n);
                }
            });
            ui.horizontal(|ui| {
                for (strip, label) in [(false, t_("Auto-split evenly", "Eşit aralıkla böl")), (true, t_("Remove karaoke tags", "Karaoke etiketlerini sil"))] {
                    if ui.button(label).clicked() {
                        app.doc.checkpoint();
                        let tag = ["\\k", "\\kf", "\\ko"][self.kara_tag];
                        let mut n = 0;
                        for &i in &app.sel {
                            if let Some(e) = app.doc.script.events.get_mut(i) {
                                let new = if strip { nsub_core::kara::strip_kara(&e.text) } else { nsub_core::kara::auto_split(&e.text, e.end - e.start, self.kara_chars, tag) };
                                if new != e.text {
                                    e.text = new;
                                    n += 1;
                                }
                            }
                        }
                        self.kara_msg = format!("{n} {}", t_("line(s) changed", "satır değişti"));
                    }
                }
            });
            ui.label(&self.kara_msg);
        });
        self.show_kara = open;
    }

    fn props(&mut self, ctx: &egui::Context, app: &mut App) {
        let mut open = self.show_props;
        if open && !self.res_loaded {
            let (w, h) = app.doc.script.resolution();
            self.res_w = w;
            self.res_h = h;
            self.res_loaded = true;
        }
        egui::Window::new(t_("Script properties", "Betik özellikleri")).open(&mut open).collapsible(false).show(ctx, |ui| {
            let keys = ["Title", "Original Script", "Translation", "Timing", "WrapStyle", "ScaledBorderAndShadow", "YCbCr Matrix"];
            egui::Grid::new("props").num_columns(2).show(ui, |ui| {
                for k in keys {
                    ui.label(k);
                    let mut v = app.doc.script.get_info(k).unwrap_or("").to_string();
                    if ui.add(egui::TextEdit::singleline(&mut v).desired_width(220.0)).changed() {
                        app.doc.touch(&format!("prop:{k}"));
                        app.doc.script.set_info(k, &v);
                    }
                    ui.end_row();
                }
            });
            ui.separator();
            ui.label(RichText::new(t_("Resolution (PlayRes)", "Çözünürlük (PlayRes)")).strong());
            ui.horizontal(|ui| {
                ui.add(egui::DragValue::new(&mut self.res_w).range(1..=16384));
                ui.label("×");
                ui.add(egui::DragValue::new(&mut self.res_h).range(1..=16384));
                if ui.button(t_("Set only", "Yalnızca ayarla")).clicked() {
                    app.doc.checkpoint();
                    app.doc.script.set_info("PlayResX", &self.res_w.to_string());
                    app.doc.script.set_info("PlayResY", &self.res_h.to_string());
                }
                if ui.button(t_("Resample (scale everything)", "Yeniden örnekle (her şeyi ölçekle)")).clicked() {
                    app.doc.checkpoint();
                    resample::resample(&mut app.doc.script, self.res_w, self.res_h, Default::default());
                    app.refresh_qc();
                }
            });
        });
        self.show_props = open;
        if !open {
            self.res_loaded = false;
        }
    }

    fn fonts(&mut self, ctx: &egui::Context, app: &mut App, fm: &nsub_fonts::FontMetrics) {
        let mut open = self.show_fonts;
        egui::Window::new(t_("Fonts used", "Kullanılan yazı tipleri")).open(&mut open).default_width(520.0).show(ctx, |ui| {
            if ui.button(t_("Scan", "Tara")).clicked() {
                self.fonts_report = fm
                    .report(&app.doc.script)
                    .found
                    .into_iter()
                    .map(|(u, p)| (format!("{}{}{}", u.family, if u.bold { " (bold)" } else { "" }, if u.italic { " (italic)" } else { "" }), true, Some(p.display().to_string())))
                    .chain(fm.report(&app.doc.script).missing.into_iter().map(|u| (format!("{}{}{}", u.family, if u.bold { " (bold)" } else { "" }, if u.italic { " (italic)" } else { "" }), false, None)))
                    .collect();
            }
            if ui.button(t_("Copy found fonts next to the script…", "Bulunan yazı tiplerini betiğin yanına kopyala…")).clicked() {
                if let Some(dir) = rfd::FileDialog::new().pick_folder() {
                    match nsub_fonts::collect_fonts(fm, &app.doc.script, &dir) {
                        Ok((rep, n)) => app.status = format!("{n} font file(s) copied, {} missing", rep.missing.len()),
                        Err(e) => app.status = format!("Error: {e}"),
                    }
                }
            }
            egui::ScrollArea::vertical().show(ui, |ui| {
                for (name, ok, path) in &self.fonts_report {
                    let text = if *ok { format!("✔ {name}  —  {}", path.clone().unwrap_or_default()) } else { format!("✘ {name}  —  {}", t_("MISSING", "EKSİK")) };
                    ui.label(if *ok { RichText::new(text) } else { RichText::new(text).color(egui::Color32::from_rgb(230, 110, 110)) });
                }
            });
        });
        self.show_fonts = open;
    }

    fn select(&mut self, ctx: &egui::Context, app: &mut App) {
        let mut open = self.show_select;
        egui::Window::new(t_("Select lines", "Satır seç")).open(&mut open).collapsible(false).show(ctx, |ui| {
            ui.text_edit_singleline(&mut self.sel_pattern);
            egui::ComboBox::from_id_salt("selfield")
                .selected_text([t_("Plain text", "Düz metin"), t_("Text with tags", "Etiketli metin"), t_("Style", "Stil"), t_("Actor", "Konuşmacı"), t_("Effect", "Efekt")][self.sel_field])
                .show_ui(ui, |ui| {
                    for (i, n) in [t_("Plain text", "Düz metin"), t_("Text with tags", "Etiketli metin"), t_("Style", "Stil"), t_("Actor", "Konuşmacı"), t_("Effect", "Efekt")].into_iter().enumerate() {
                        ui.selectable_value(&mut self.sel_field, i, n);
                    }
                });
            ui.horizontal(|ui| {
                ui.checkbox(&mut self.sel_regex, tr("Regex"));
                ui.checkbox(&mut self.sel_case, tr("Match case"));
                ui.checkbox(&mut self.sel_invert, t_("Invert", "Tersine çevir"));
            });
            if ui.button(t_("Select", "Seç")).clicked() {
                let field = [ops::Field::PlainText, ops::Field::Text, ops::Field::Style, ops::Field::Actor, ops::Field::Effect][self.sel_field];
                match ops::select_matching(&app.doc.script, &self.sel_pattern, field, self.sel_regex, self.sel_case, self.sel_invert, true) {
                    Ok(hits) => {
                        self.sel_msg = format!("{} {}", hits.len(), t_("line(s) selected", "satır seçildi"));
                        app.sel = hits.iter().copied().collect();
                        app.active = hits.first().copied().or(app.active);
                    }
                    Err(e) => self.sel_msg = e,
                }
            }
            ui.label(&self.sel_msg);
        });
        self.show_select = open;
    }

    fn fix(&mut self, ctx: &egui::Context, app: &mut App) {
        let mut open = self.show_fix;
        egui::Window::new(t_("Fix common errors", "Yaygın hataları düzelt")).open(&mut open).collapsible(false).show(ctx, |ui| {
            ui.checkbox(&mut self.fix_overlaps, t_("Fix overlapping times", "Çakışan zamanları düzelt"));
            ui.checkbox(&mut self.fix_min_dur, t_("Extend too-short lines to the minimum duration", "Çok kısa satırları minimum süreye uzat"));
            ui.checkbox(&mut self.fix_cps, t_("Lower CPS by extending the end time", "CPS'yi bitiş süresini uzatarak düşür"));
            ui.checkbox(&mut self.fix_spaces, t_("Remove double / surrounding spaces", "Çift / baştaki-sondaki boşlukları sil"));
            ui.checkbox(&mut self.fix_hi, t_("Remove [sound effects], (HI), ♪ music ♪, NAME: labels", "[ses efekti], (işitme), ♪ müzik ♪, AD: etiketlerini sil"));
            ui.checkbox(&mut self.fix_dups, t_("Merge identical consecutive lines", "Aynı ardışık satırları birleştir"));
            ui.checkbox(&mut self.fix_unbreak, t_("Remove line breaks (\\N → space)", "Satır sonlarını kaldır (\\N → boşluk)"));
            ui.horizontal(|ui| {
                ui.checkbox(&mut self.fix_balance, t_("Auto-balance lines longer than", "Şundan uzun satırları dengele:"));
                ui.add(egui::DragValue::new(&mut self.fix_balance_len).range(10..=100));
            });
            ui.horizontal(|ui| {
                ui.label(t_("Change case", "Büyük/küçük harf"));
                egui::ComboBox::from_id_salt("fixcase")
                    .selected_text(["—", "UPPER / BÜYÜK", "lower / küçük", "Sentence / Cümle", "Title / Başlık"][self.fix_case])
                    .show_ui(ui, |ui| {
                        for (i, n) in ["—", "UPPER / BÜYÜK", "lower / küçük", "Sentence / Cümle", "Title / Başlık"].into_iter().enumerate() {
                            ui.selectable_value(&mut self.fix_case, i, n);
                        }
                    });
            });
            ui.checkbox(&mut self.fix_only_selected, t_("Only for selected lines (casing)", "Yalnızca seçili satırlar (harf değişimi)"));
            if ui.button(tr("Apply")).clicked() {
                let cfg = app.qc_cfg.clone();
                app.doc.checkpoint();
                let mut n = 0;
                let sel: Vec<usize> = app.sel.iter().copied().collect();
                let only = if self.fix_only_selected { Some(sel.as_slice()) } else { None };
                if self.fix_hi { n += fixes::remove_hearing_impaired(&mut app.doc.script) }
                if self.fix_unbreak { n += fixes::unbreak(&mut app.doc.script) }
                if self.fix_balance { n += fixes::balance_all(&mut app.doc.script, self.fix_balance_len) }
                if self.fix_spaces { n += nsub_core::qc::fix_double_spaces(&mut app.doc.script) }
                if self.fix_dups { n += fixes::merge_duplicates(&mut app.doc.script, 300) }
                if self.fix_min_dur { n += nsub_core::qc::fix_min_duration(&mut app.doc.script, cfg.min_duration_ms) }
                if self.fix_cps { n += fixes::fix_cps(&mut app.doc.script, cfg.max_cps, cfg.min_gap_ms, cfg.max_duration_ms) }
                if self.fix_overlaps { n += fixes::apply_min_gap(&mut app.doc.script, cfg.min_gap_ms) }
                match self.fix_case {
                    1 => n += fixes::change_case(&mut app.doc.script, fixes::Case::Upper, only),
                    2 => n += fixes::change_case(&mut app.doc.script, fixes::Case::Lower, only),
                    3 => n += fixes::change_case(&mut app.doc.script, fixes::Case::Sentence, only),
                    4 => n += fixes::change_case(&mut app.doc.script, fixes::Case::Title, only),
                    _ => {}
                }
                self.fix_msg = format!("{n} {}", t_("change(s) applied", "değişiklik uygulandı"));
                let len = app.doc.script.events.len();
                app.sel.retain(|&i| i < len);
                if app.active.map_or(false, |a| a >= len) {
                    app.active = len.checked_sub(1);
                }
                app.refresh_qc();
            }
            ui.label(&self.fix_msg);
        });
        self.show_fix = open;
    }

    fn post(&mut self, ctx: &egui::Context, app: &mut App) {
        let mut open = self.show_post;
        egui::Window::new(t_("Timing post-processor", "Zamanlama son işlemcisi")).open(&mut open).collapsible(false).show(ctx, |ui| {
            egui::Grid::new("post").num_columns(2).show(ui, |ui| {
                ui.label(t_("Lead-in (ms)", "Öne alma (ms)"));
                ui.add(egui::DragValue::new(&mut self.lead_in).speed(5.0));
                ui.end_row();
                ui.label(t_("Lead-out (ms)", "Geri bırakma (ms)"));
                ui.add(egui::DragValue::new(&mut self.lead_out).speed(5.0));
                ui.end_row();
            });
            if ui.button(tr("Apply")).clicked() {
                app.doc.checkpoint();
                let n = ops::lead_in_out(&mut app.doc.script, self.lead_in, self.lead_out);
                self.post_msg = format!("{n} {}", t_("line(s) changed", "satır değişti"));
                app.refresh_qc();
            }
            ui.label(&self.post_msg);
        });
        self.show_post = open;
    }
}
