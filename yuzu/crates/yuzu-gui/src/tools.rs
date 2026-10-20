//! Osi's tools in the editor: reference subtitle with offset (grid column, transfer),
//! source ↔ candidate comparison (aligncompare), piecewise sync to speech or to the reference
//! (bsync / bsnap) and the term glossary (termcheck).

use super::*;
use asscore::compare::{self, CompareOptions, CompareResult};
use asscore::glossary::{self, Glossary};
use asscore::sync::{self, Segment, SyncOptions};
use asscore::transfer::{self, RefIndex, TransferOptions};
use std::sync::mpsc::{channel, Receiver};

pub(super) struct SyncPlan {
    pub segs: Vec<Segment>,
    pub removed: Vec<(i64, i64, String)>,
    pub changes: Vec<(usize, i64, i64)>,
    pub warnings: Vec<String>,
    /// Document generation the plan was made for (stale plans are not applied).
    pub gen: u64,
}

pub(super) struct OsiTools {
    pub reference: Option<Script>,
    pub reference_path: Option<PathBuf>,
    pub ref_offset_ms: i64,
    pub ref_index: Option<RefIndex>,
    pub show_ref_column: bool,
    pub ref_msg: String,
    pub cmp_opts: CompareOptions,
    pub cmp_auto: bool,
    pub cmp_manual_s: f64,
    pub cmp_source: Option<(PathBuf, Script)>,
    pub cmp_result: Option<CompareResult>,
    pub cmp_filter: Option<&'static str>,
    pub cmp_only_flagged: bool,
    pub cmp_msg: String,
    pub sync_use_ref: bool,
    pub sync_sel_only: bool,
    pub sync_opts: SyncOptions,
    pub sync_plan: Option<SyncPlan>,
    pub sync_rx: Option<Receiver<Result<SyncPlan, String>>>,
    pub sync_msg: String,
    pub glossary: Option<(PathBuf, Glossary)>,
    pub gl_issues: Vec<(usize, String, bool)>,
    pub gl_msg: String,
    pub tr_opts: TransferOptions,
    pub tr_preview: Option<transfer::Plan>,
}

impl Default for OsiTools {
    fn default() -> Self {
        OsiTools {
            reference: None, reference_path: None, ref_offset_ms: 0, ref_index: None, show_ref_column: true, ref_msg: String::new(),
            cmp_opts: CompareOptions::default(), cmp_auto: true, cmp_manual_s: 0.0, cmp_source: None, cmp_result: None, cmp_filter: None, cmp_only_flagged: true, cmp_msg: String::new(),
            sync_use_ref: false, sync_sel_only: false, sync_opts: SyncOptions::default(), sync_plan: None, sync_rx: None, sync_msg: String::new(),
            glossary: None, gl_issues: vec![], gl_msg: String::new(), tr_opts: TransferOptions::default(), tr_preview: None,
        }
    }
}

pub(super) fn file_name(p: &std::path::Path) -> String { p.file_name().map(|s| s.to_string_lossy().to_string()).unwrap_or_default() }

pub(super) fn read_subtitle(p: &std::path::Path) -> Option<Script> {
    let b = std::fs::read(p).ok()?;
    let txt = asscore::decode_bytes(&b);
    Some(asscore::formats::load_any(&p.to_string_lossy(), &txt, &Default::default()).unwrap_or_else(|| asscore::load_str(&p.to_string_lossy(), &txt)))
}

impl App {
    // ------------------------------------------------------------ reference

    pub(super) fn set_reference(&mut self, path: PathBuf) {
        match read_subtitle(&path) {
            Some(sc) => {
                self.osi.ref_index = Some(RefIndex::new(&sc.events));
                self.osi.reference = Some(sc);
                self.osi.reference_path = Some(path.clone());
                self.osi.ref_offset_ms = 0;
                self.osi.ref_msg = tf!("Reference: {}", file_name(&path));
                self.show_panel(Panel::Reference);
            }
            None => self.last_status = tf!("Open failed: {}", path.display()),
        }
    }

    pub(super) fn close_reference(&mut self) {
        self.osi.reference = None;
        self.osi.reference_path = None;
        self.osi.ref_index = None;
        self.osi.tr_preview = None;
    }

    /// Reference text overlapping document line `i` (offset applied).
    pub(super) fn reference_text(&self, i: usize) -> Option<String> {
        let (r, idx) = (self.osi.reference.as_ref()?, self.osi.ref_index.as_ref()?);
        let e = self.doc.events.get(i)?;
        idx.best(&r.events, e.start, e.end, self.osi.ref_offset_ms).map(|(j, _)| tags::display_text(&r.events[j].text).replace('\n', " "))
    }

    pub(super) fn find_reference_offset(&mut self) {
        let Some(r) = &self.osi.reference else { return };
        let (shift, score) = compare::shift_between(&r.events, &self.doc.events);
        if score < 0.35 {
            self.osi.ref_msg = tf!("No clear offset found (confidence {}%) — not applied", (score * 100.0) as i64);
        } else {
            self.osi.ref_offset_ms = shift;
            self.osi.ref_msg = tf!("Offset {} s (confidence {}%)", format!("{:+.2}", shift as f64 / 1000.0), (score * 100.0) as i64);
        }
    }

    pub(super) fn reference_ui(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            match &self.osi.reference_path {
                Some(p) => { ui.label(RichText::new(file_name(p)).strong()); }
                None => { ui.weak(t("No reference loaded")); }
            }
            if ui.button(t("Load…")).clicked() {
                if let Some(p) = rfd::FileDialog::new().add_filter(t("Subtitles"), &["ass", "ssa", "srt", "vtt", "sbv", "ttml", "dfxp", "sub"]).pick_file() { self.set_reference(p); }
            }
            if self.osi.reference.is_some() && ui.button(t("Close")).clicked() { self.close_reference(); }
        });
        if self.osi.reference.is_none() { return; }
        ui.horizontal(|ui| {
            ui.label(t("Offset"));
            let mut s = self.osi.ref_offset_ms as f64 / 1000.0;
            if ui.add(egui::DragValue::new(&mut s).speed(0.01).range(-600.0..=600.0).suffix(" s").fixed_decimals(2)).on_hover_text(t("How much later the document is than the reference")).changed() {
                self.osi.ref_offset_ms = (s * 1000.0).round() as i64;
            }
            if ui.button(t("Find offset")).clicked() { self.find_reference_offset(); }
            ui.checkbox(&mut self.osi.show_ref_column, t("Reference column in the grid"));
        });
        if !self.osi.ref_msg.is_empty() { ui.weak(&self.osi.ref_msg); }
        ui.separator();
        ui.strong(t("Transfer the reference text into the document"));
        ui.weak(t("Each reference line goes to the document line it overlaps most; unmatched document lines stay, unmatched reference lines are added."));
        let o = &mut self.osi.tr_opts;
        ui.horizontal(|ui| {
            ui.label(t("Min overlap"));
            ui.add(egui::DragValue::new(&mut o.min_overlap).range(0.05..=1.0).speed(0.01));
            ui.checkbox(&mut o.keep_leading_tags, t("Keep leading tags"));
            ui.checkbox(&mut o.keep_original, t("Keep the original as {EN: …}"));
        });
        ui.horizontal(|ui| {
            ui.checkbox(&mut o.use_ref_timing, t("Use the reference's times"));
            ui.checkbox(&mut o.insert_orphans, t("Add unmatched reference lines"));
        });
        ui.horizontal(|ui| {
            if ui.button(t("Preview")).clicked() { self.transfer_preview(); }
            if ui.add_enabled(self.osi.tr_preview.is_some(), egui::Button::new(t("Apply"))).clicked() { self.transfer_apply(); }
        });
        if let Some(p) = &self.osi.tr_preview {
            ui.label(tf!("{} lines filled · {} added · {} without reference · {} notes", p.filled, p.orphans, p.no_ref, p.issues.len()));
            let mut goto = None;
            egui::ScrollArea::vertical().max_height(200.0).id_salt("tr_issues").show(ui, |ui| {
                for is in &p.issues {
                    ui.horizontal(|ui| {
                        if let Some(r) = is.row { if ui.link(format!("#{}", r + 1)).clicked() { goto = Some(r); } }
                        ui.label(is.kind.label());
                        ui.weak(&is.detail);
                    });
                }
            });
            if let Some(g) = goto { if g < self.doc.events.len() { self.select_only(g); self.scroll_to = Some(g); } }
        }
    }

    fn reference_shifted(&self) -> Vec<Event> {
        let Some(r) = &self.osi.reference else { return vec![] };
        // reference times on the document's time line
        r.events.iter().map(|e| { let mut e = e.clone(); e.start += self.osi.ref_offset_ms; e.end += self.osi.ref_offset_ms; e }).collect()
    }

    fn transfer_preview(&mut self) {
        let refs = self.reference_shifted();
        self.osi.tr_preview = Some(transfer::plan_transfer(&self.doc.events, &refs, &self.osi.tr_opts));
    }

    fn transfer_apply(&mut self) {
        let Some(p) = self.osi.tr_preview.take() else { return };
        self.push_undo();
        let n = p.filled;
        self.doc.events = p.events;
        self.clamp_sel();
        self.run_qc();
        self.last_status = tf!("{} lines filled from the reference", n);
    }

    // ------------------------------------------------------------ comparison (aligncompare)

    fn compare_source(&self) -> Option<(String, Vec<Event>)> {
        match &self.osi.cmp_source {
            Some((p, sc)) => Some((p.to_string_lossy().to_string(), sc.events.clone())),
            None => self.osi.reference.as_ref().map(|r| (self.osi.reference_path.as_ref().map(|p| p.to_string_lossy().to_string()).unwrap_or_default(), r.events.clone())),
        }
    }

    pub(super) fn run_compare(&mut self) {
        let Some((_, src)) = self.compare_source() else { self.osi.cmp_msg = t("Choose a source file or load a reference first").into(); return };
        let mut o = self.osi.cmp_opts.clone();
        o.shift_ms = if self.osi.cmp_auto { None } else { Some((self.osi.cmp_manual_s * 1000.0).round() as i64) };
        let r = compare::compare(&src, &self.doc.events, &o);
        self.osi.cmp_msg = String::new();
        self.osi.cmp_result = Some(r);
    }

    pub(super) fn compare_ui(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.label(t("Source"));
            let src_name = match &self.osi.cmp_source { Some((p, _)) => file_name(p), None => self.osi.reference_path.as_ref().map(|p| format!("{} ({})", file_name(p), t("reference"))).unwrap_or_else(|| "—".into()) };
            ui.label(RichText::new(src_name).strong());
            if ui.button(t("Choose…")).clicked() {
                if let Some(p) = rfd::FileDialog::new().add_filter(t("Subtitles"), &["ass", "ssa", "srt", "vtt", "sbv", "ttml", "dfxp"]).pick_file() {
                    if let Some(sc) = read_subtitle(&p) { self.osi.cmp_source = Some((p, sc)); }
                }
            }
            if self.osi.cmp_source.is_some() && self.osi.reference.is_some() && ui.small_button(t("Use the reference")).clicked() { self.osi.cmp_source = None; }
            ui.weak(t("candidate = this document"));
        });
        let o = &mut self.osi.cmp_opts;
        ui.horizontal(|ui| {
            ui.radio_value(&mut self.osi.cmp_auto, true, t("Find the shift (±15 s)"));
            ui.radio_value(&mut self.osi.cmp_auto, false, t("Shift"));
            ui.add_enabled(!self.osi.cmp_auto, egui::DragValue::new(&mut self.osi.cmp_manual_s).speed(0.01).suffix(" s").fixed_decimals(2));
        });
        ui.horizontal(|ui| {
            ui.label(t("Length ratio"));
            ui.add(egui::DragValue::new(&mut o.ratio_min).range(0.05..=1.0).speed(0.01));
            ui.label("–");
            ui.add(egui::DragValue::new(&mut o.ratio_max).range(1.0..=10.0).speed(0.05));
            ui.label(t("Similarity"));
            ui.add(egui::DragValue::new(&mut o.sim).range(0.5..=1.01).speed(0.01));
            ui.label(t("Start difference"));
            let mut d = o.max_start_diff_ms as f64 / 1000.0;
            if ui.add(egui::DragValue::new(&mut d).range(0.05..=5.0).speed(0.01).suffix(" s")).changed() { o.max_start_diff_ms = (d * 1000.0) as i64; }
        });
        ui.horizontal(|ui| {
            ui.checkbox(&mut o.include_typeset, t("Include signs / typesetting"));
            ui.label(t("Skip styles (source)"));
            ui.add(egui::TextEdit::singleline(&mut o.skip_styles_src).desired_width(90.0).hint_text("regex"));
            ui.label(t("(candidate)"));
            ui.add(egui::TextEdit::singleline(&mut o.skip_styles_cand).desired_width(90.0).hint_text("regex"));
        });
        ui.horizontal(|ui| {
            if ui.button(RichText::new(t("Compare")).strong()).clicked() { self.run_compare(); }
            if !self.osi.cmp_msg.is_empty() { ui.colored_label(Color32::from_rgb(235, 190, 80), &self.osi.cmp_msg); }
        });
        let Some(res) = &self.osi.cmp_result else { return };
        ui.separator();
        let src_name = self.compare_source().map(|s| s.0).unwrap_or_default();
        let cand_name = self.path.as_ref().map(|p| p.to_string_lossy().to_string()).unwrap_or_else(|| t("Untitled").into());
        for line in compare::summary_lines(res, &file_name(std::path::Path::new(&src_name)), &file_name(std::path::Path::new(&cand_name))) {
            if line.starts_with("UYARI") { ui.colored_label(Color32::from_rgb(235, 190, 80), line); } else { ui.weak(line); }
        }
        let counts = res.counts();
        let mut filter = self.osi.cmp_filter;
        ui.horizontal_wrapped(|ui| {
            if ui.selectable_label(filter.is_none() && !self.osi.cmp_only_flagged, tf!("All ({})", res.groups.len())).clicked() { filter = None; self.osi.cmp_only_flagged = false; }
            if ui.selectable_label(filter.is_none() && self.osi.cmp_only_flagged, tf!("Flagged ({})", res.flagged().len())).clicked() { filter = None; self.osi.cmp_only_flagged = true; }
            for code in compare::FLAG_ORDER {
                let n = counts.get(code).copied().unwrap_or(0);
                if n > 0 && ui.selectable_label(filter == Some(code), format!("{} ({})", code, n)).on_hover_text(compare::flag_help(code)).clicked() { filter = Some(code); }
            }
        });
        self.osi.cmp_filter = filter;
        let mut goto: Option<Vec<usize>> = None;
        let shown: Vec<usize> = res.groups.iter().enumerate()
            .filter(|(_, g)| match filter { Some(code) => g.codes().contains(&code), None => !self.osi.cmp_only_flagged || !g.flags.is_empty() })
            .map(|(k, _)| k).collect();
        let col = |c: &str| match c { "EKSİK" | "FAZLA" | "BOŞ" | "İNGİLİZCE?" => Color32::from_rgb(255, 110, 100), "UZUNLUK" | "ZAMAN" => Color32::from_rgb(235, 190, 80), _ => Color32::from_rgb(130, 170, 255) };
        // one line per group, clipped; the whole text is in the tooltip
        ui.push_id("cmp_rows", |ui| {
            TableBuilder::new(ui)
                .striped(true)
                .max_scroll_height(320.0)
                .auto_shrink([false, true])
                .column(Column::exact(34.0))
                .column(Column::exact(34.0))
                .column(Column::remainder().at_least(140.0).clip(true))
                .column(Column::remainder().at_least(140.0).clip(true))
                .column(Column::initial(170.0).at_least(80.0).clip(true))
                .header(20.0, |mut h| {
                    for name in ["#", t("Kind"), t("Source"), t("Candidate"), t("Flags")] { h.col(|ui| { ui.strong(name); }); }
                })
                .body(|body| body.rows(20.0, shown.len(), |mut row| {
                    let k = shown[row.index()];
                    let g = &res.groups[k];
                    row.col(|ui| {
                        let rows: Vec<usize> = g.cand.iter().map(|c| c.row).collect();
                        if ui.link(format!("{}", k + 1)).on_hover_text(t("Select the candidate lines")).clicked() && !rows.is_empty() { goto = Some(rows); }
                    });
                    row.col(|ui| { ui.label(g.kind()); });
                    row.col(|ui| { ui.add(egui::Label::new(g.src_text(" / ")).truncate()).on_hover_text(g.src_text("\n")); });
                    row.col(|ui| {
                        let cand: Vec<String> = g.cand.iter().map(|c| format!("{}: {}", c.row + 1, if c.plain.is_empty() { "∅".to_string() } else { c.plain.clone() })).collect();
                        ui.add(egui::Label::new(cand.join(" / ")).truncate()).on_hover_text(cand.join("\n"));
                    });
                    row.col(|ui| {
                        ui.horizontal(|ui| { for f in &g.flags { ui.colored_label(col(f.code), f.label()); } })
                            .response.on_hover_text(g.flags.iter().map(|f| format!("{} — {}", f.label(), compare::flag_help(f.code))).collect::<Vec<_>>().join("\n"));
                    });
                }));
        });
        let (flagged_rows, missing): (Vec<usize>, usize) = (res.flags_by_cand_row().keys().copied().collect(), res.counts().get("EKSİK").copied().unwrap_or(0));
        let shift = res.shift_ms;
        ui.horizontal_wrapped(|ui| {
            if ui.button(tf!("Select flagged lines ({})", flagged_rows.len())).clicked() && !flagged_rows.is_empty() {
                self.selection = flagged_rows.iter().copied().filter(|&r| r < self.doc.events.len()).collect();
                if let Some(&f) = flagged_rows.first() { self.active = f; self.scroll_to = Some(f); }
            }
            if ui.add_enabled(missing > 0, egui::Button::new(tf!("Add missing lines as comments ({})", missing))).clicked() { self.compare_add_missing(); }
            if self.osi.reference.is_some() && self.osi.cmp_source.is_none() && ui.button(t("Apply the shift to the reference")).clicked() {
                self.osi.ref_offset_ms = shift;
                self.osi.ref_msg = tf!("Offset {} s (confidence {}%)", format!("{:+.2}", shift as f64 / 1000.0), (self.osi.cmp_result.as_ref().map(|r| r.shift_score).unwrap_or(0.0) * 100.0) as i64);
            }
        });
        ui.horizontal(|ui| {
            if ui.button(t("HTML report…")).clicked() { self.compare_export("html"); }
            if ui.button(t("CSV…")).clicked() { self.compare_export("csv"); }
            if ui.button(t("Marked copy…")).clicked() { self.compare_export("marked"); }
        });
        if let Some(rows) = goto {
            self.selection = rows.iter().copied().filter(|&r| r < self.doc.events.len()).collect();
            if let Some(&f) = rows.first() { if f < self.doc.events.len() { self.active = f; self.anchor = f; self.scroll_to = Some(f); } }
        }
    }

    /// Insert the EKSİK source lines as comments at their time (once; repeated runs add nothing).
    pub(super) fn compare_add_missing(&mut self) {
        let Some(res) = &self.osi.cmp_result else { return };
        let new = compare::missing_as_comments(&self.doc, res);
        let existing: std::collections::HashSet<(i64, String)> = self.doc.events.iter().map(|e| (e.start, e.text.clone())).collect();
        let new: Vec<Event> = new.into_iter().filter(|e| !existing.contains(&(e.start, e.text.clone()))).collect();
        if new.is_empty() { self.osi.cmp_msg = t("Nothing to add").into(); return; }
        self.push_undo();
        let n = new.len();
        for e in new {
            let pos = self.doc.events.iter().position(|x| x.start > e.start).unwrap_or(self.doc.events.len());
            self.doc.events.insert(pos, e);
        }
        self.clamp_sel();
        self.osi.cmp_msg = tf!("{} missing lines added as comments", n);
    }

    fn compare_export(&mut self, kind: &str) {
        let Some(res) = &self.osi.cmp_result else { return };
        let src = self.compare_source().map(|s| s.0).unwrap_or_default();
        let cand = self.path.as_ref().map(|p| p.to_string_lossy().to_string()).unwrap_or_else(|| "aday.ass".into());
        let stem = self.path.as_ref().and_then(|p| p.file_stem()).map(|s| s.to_string_lossy().to_string()).unwrap_or_else(|| "aday".into());
        let (name, filter, ext) = match kind { "html" => (format!("{}_karsilastirma.html", stem), "HTML", "html"), "csv" => (format!("{}_karsilastirma.csv", stem), "CSV", "csv"), _ => (format!("{}_isaretli.ass", stem), "ASS", "ass") };
        let Some(p) = rfd::FileDialog::new().add_filter(filter, &[ext]).set_file_name(name).save_file() else { return };
        if self.path.as_ref() == Some(&p) { self.osi.cmp_msg = t("The marked copy cannot overwrite the document").into(); return; }
        let data = match kind {
            "html" => compare::to_html(res, &src, &cand, self.osi.cmp_only_flagged).into_bytes(),
            "csv" => compare::to_csv(res, self.osi.cmp_only_flagged).into_bytes(),
            _ => format!("\u{feff}{}", ass::write(&compare::marked_document(&self.doc, res))).into_bytes(),
        };
        self.osi.cmp_msg = match std::fs::write(&p, data) { Ok(_) => tf!("Saved {}", p.display()), Err(e) => tf!("Save failed: {}", e) };
    }

    // ------------------------------------------------------------ piecewise sync (bsync / bsnap)

    fn sync_rows(&self) -> Option<Vec<usize>> {
        (self.osi.sync_sel_only && self.selection.len() > 1).then(|| self.sel_vec())
    }

    pub(super) fn start_sync_plan(&mut self) {
        let mut o = self.osi.sync_opts.clone();
        o.rows = self.sync_rows();
        let events = self.doc.events.clone();
        let gen = self.doc_gen;
        let (tx, rx) = channel();
        if self.osi.sync_use_ref {
            let Some(r) = &self.osi.reference else { self.osi.sync_msg = t("Load a correctly timed reference first").into(); return };
            let refs = r.events.clone();
            o.oped_max_ms = 0;
            let total = sync::reference_total_ms(&refs, &events, 60_000);
            let act = sync::activity_from_reference(&refs, total, &o.oped_regex);
            let (segs, removed) = sync::plan(&events, &act, &o, |_| {});
            let (changes, warnings) = sync::changes(&events, &segs, 1_000_000_000);
            self.osi.sync_plan = Some(SyncPlan { segs, removed, changes, warnings, gen });
            self.osi.sync_msg = String::new();
            return;
        }
        if self.media.pcm.is_empty() { self.osi.sync_msg = t("Open the video or audio first (the speech is read from it)").into(); return; }
        if self.media.peaks_loading { self.osi.sync_msg = t("Wait until the audio is decoded").into(); return; }
        let pcm = self.media.pcm.clone();
        let ctx = self.ctx.clone();
        std::thread::spawn(move || {
            let act = yuzu_media::vad::speech_activity(&pcm, yuzu_media::audio::RATE);
            let total = pcm.len() as i64 * 1000 / yuzu_media::audio::RATE as i64;
            let (segs, removed) = sync::plan(&events, &act, &o, |_| {});
            let (changes, warnings) = sync::changes(&events, &segs, total);
            let _ = tx.send(Ok(SyncPlan { segs, removed, changes, warnings, gen }));
            ctx.request_repaint();
        });
        self.osi.sync_rx = Some(rx);
        self.osi.sync_msg = t("Detecting speech…").into();
    }

    pub(super) fn poll_sync(&mut self) {
        if let Some(rx) = &self.osi.sync_rx {
            if let Ok(r) = rx.try_recv() {
                self.osi.sync_rx = None;
                match r { Ok(p) => { self.osi.sync_plan = Some(p); self.osi.sync_msg.clear(); } Err(e) => self.osi.sync_msg = e }
            }
        }
    }

    pub(super) fn apply_sync(&mut self) {
        let Some(p) = &self.osi.sync_plan else { return };
        if p.gen != self.doc_gen { self.osi.sync_msg = t("The document changed since planning — plan again").into(); return; }
        let ch = p.changes.clone();
        if ch.is_empty() { self.osi.sync_msg = t("Nothing to change").into(); return; }
        self.push_undo();
        for (r, s, e) in &ch { if let Some(ev) = self.doc.events.get_mut(*r) { ev.start = *s; ev.end = *e; } }
        if self.osi.sync_use_ref { self.osi.ref_offset_ms = 0; }
        self.buf_key = (usize::MAX, 0, 0);
        self.run_qc();
        self.osi.sync_plan = None;
        self.last_status = tf!("{} lines moved", ch.len());
    }

    pub(super) fn sync_ui(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.label(t("Time source"));
            ui.radio_value(&mut self.osi.sync_use_ref, false, t("Speech in the audio"));
            ui.radio_value(&mut self.osi.sync_use_ref, true, t("Reference subtitle"));
        });
        ui.weak(t("The offset only changes at scene breaks; signs, screen text and OP/ED move with their scene but are not used for alignment."));
        let o = &mut self.osi.sync_opts;
        ui.horizontal(|ui| {
            ui.label(t("Scene break ≥"));
            let mut g = o.scene_gap_ms as f64 / 1000.0;
            if ui.add(egui::DragValue::new(&mut g).range(1.0..=30.0).speed(0.1).suffix(" s")).changed() { o.scene_gap_ms = (g * 1000.0) as i64; }
            ui.label(t("Search ±"));
            let mut f = o.fine_range_ms as f64 / 1000.0;
            if ui.add(egui::DragValue::new(&mut f).range(1.0..=30.0).speed(0.1).suffix(" s")).changed() { o.fine_range_ms = (f * 1000.0) as i64; }
            ui.checkbox(&mut self.osi.sync_sel_only, t("Selected lines only"));
        });
        ui.horizontal(|ui| {
            ui.label(t("OP/ED styles"));
            ui.add(egui::TextEdit::singleline(&mut o.oped_regex).desired_width(320.0));
        });
        ui.horizontal(|ui| {
            let running = self.osi.sync_rx.is_some();
            if ui.add_enabled(!running, egui::Button::new(RichText::new(t("Plan")).strong())).clicked() { self.start_sync_plan(); }
            if running { ui.spinner(); }
            if !self.osi.sync_msg.is_empty() { ui.colored_label(Color32::from_rgb(235, 190, 80), &self.osi.sync_msg); }
        });
        let Some(p) = &self.osi.sync_plan else { return };
        ui.separator();
        let moved = p.changes.len();
        ui.label(tf!("{} scenes · {} lines will move", p.segs.len(), moved));
        for w in &p.warnings { ui.colored_label(Color32::from_rgb(235, 190, 80), w); }
        for (s, e, why) in &p.removed { ui.weak(format!("{} – {}: {}", time::format_ass(*s), time::format_ass(*e), why)); }
        let mut goto = None;
        egui::ScrollArea::vertical().max_height(300.0).id_salt("sync_segs").show(ui, |ui| {
            egui::Grid::new("sync_grid").striped(true).num_columns(5).spacing([12.0, 4.0]).show(ui, |ui| {
                ui.strong(t("Scene")); ui.strong(t("Lines")); ui.strong(t("Offset"));
                ui.strong(t("Margin")).on_hover_text(t("How much better the chosen offset fits than the best one at least 0.3 s away, as a share of the scene's anchor speech. Under 5% is doubtful."));
                ui.strong(t("Note")); ui.end_row();
                for (k, s) in p.segs.iter().enumerate() {
                    if ui.link(format!("{} · {}", k + 1, time::format_ass(s.start))).clicked() { goto = s.rows.first().copied(); }
                    ui.label(format!("{} ({})", s.rows.len(), s.anchors.len()));
                    let col = if s.shift == 0 { ui.visuals().weak_text_color() } else { ui.visuals().text_color() };
                    ui.colored_label(col, format!("{:+.2} s", s.shift as f64 / 1000.0));
                    let c = (s.confidence * 100.0).min(999.0);
                    ui.colored_label(if s.anchors.is_empty() { Color32::GRAY } else if c < 5.0 { Color32::from_rgb(235, 190, 80) } else { Color32::from_rgb(120, 200, 140) }, format!("{:.0}%", c));
                    ui.weak(s.notes.join("; "));
                    ui.end_row();
                }
            });
        });
        if let Some(r) = goto { self.select_only(r); self.scroll_to = Some(r); }
        if ui.button(RichText::new(tf!("Apply ({} lines)", moved)).strong()).clicked() { self.apply_sync(); }
    }

    // ------------------------------------------------------------ glossary (termcheck)

    /// Load the glossary next to the script (terimler.tsv / glossary.tsv) or from the config folder.
    pub(super) fn autoload_glossary(&mut self) {
        if self.osi.glossary.is_some() { return; }
        for p in glossary::default_paths(self.path.as_deref(), &crate::settings::config_dir()) {
            if p.exists() { if let Ok(g) = Glossary::load(&p) { self.osi.glossary = Some((p, g)); return; } }
        }
    }

    pub(super) fn glossary_issues(&self) -> Vec<(usize, String, bool)> {
        let Some((_, g)) = &self.osi.glossary else { return vec![] };
        glossary::check(g, &self.doc.events, |i| self.reference_text(i))
    }

    pub(super) fn glossary_ui(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            match &self.osi.glossary {
                Some((p, g)) => { ui.label(RichText::new(file_name(p)).strong()); ui.weak(tf!("{} terms", g.terms.len())); }
                None => { ui.weak(t("No glossary (terimler.tsv next to the script is loaded automatically)")); }
            }
            if ui.button(t("Open…")).clicked() {
                if let Some(p) = rfd::FileDialog::new().add_filter("TSV", &["tsv", "txt"]).pick_file() {
                    match Glossary::load(&p) { Ok(g) => self.osi.glossary = Some((p, g)), Err(e) => self.osi.gl_msg = e.to_string() }
                }
            }
            if let Some((p, _)) = self.osi.glossary.clone() { if ui.button(t("Reload")).clicked() { if let Ok(g) = Glossary::load(&p) { self.osi.glossary = Some((p, g)); } } }
        });
        if self.osi.glossary.is_none() { return; }
        ui.horizontal(|ui| {
            if ui.button(RichText::new(t("Check")).strong()).clicked() { self.osi.gl_issues = self.glossary_issues(); self.osi.gl_msg = tf!("{} findings", self.osi.gl_issues.len()); }
            let fixable = self.osi.gl_issues.iter().filter(|x| x.2).count();
            if ui.add_enabled(fixable > 0, egui::Button::new(tf!("Fix forbidden terms ({})", fixable))).clicked() { self.glossary_fix(None); }
            if !self.osi.gl_msg.is_empty() { ui.weak(&self.osi.gl_msg); }
        });
        let mut goto = None;
        let mut fix_one = None;
        egui::ScrollArea::vertical().max_height(320.0).id_salt("gl_issues").show(ui, |ui| {
            for (i, (row, msg, fixable)) in self.osi.gl_issues.iter().enumerate() {
                ui.horizontal(|ui| {
                    if ui.link(format!("#{}", row + 1)).clicked() { goto = Some(*row); }
                    ui.colored_label(if *fixable { Color32::from_rgb(255, 110, 100) } else { Color32::from_rgb(235, 190, 80) }, msg);
                    if *fixable && ui.small_button(t("Fix")).clicked() { fix_one = Some((i, *row)); }
                });
            }
        });
        if let Some(g) = goto { if g < self.doc.events.len() { self.select_only(g); self.scroll_to = Some(g); } }
        if let Some((_, row)) = fix_one { self.glossary_fix(Some(row)); }
    }

    /// Replace forbidden variants (one line or all lines) by the preferred terms.
    pub(super) fn glossary_fix(&mut self, row: Option<usize>) {
        let Some((_, g)) = &self.osi.glossary else { return };
        let rows: Vec<usize> = match row { Some(r) => vec![r], None => self.osi.gl_issues.iter().filter(|x| x.2).map(|x| x.0).collect() };
        let mut fixed: Vec<(usize, String)> = Vec::new();
        for r in rows {
            if let Some(e) = self.doc.events.get(r) {
                // fix only the text outside override blocks
                let parts = tags::split_parts(&e.text);
                let new: Vec<tags::Part> = parts.into_iter().map(|p| match p { tags::Part::Text(t) => tags::Part::Text(g.fix_forbidden(&t)), b => b }).collect();
                let txt = tags::join_parts(&new);
                if txt != e.text && !fixed.iter().any(|(x, _)| *x == r) { fixed.push((r, txt)); }
            }
        }
        if fixed.is_empty() { return; }
        self.push_undo();
        let n = fixed.len();
        for (r, txt) in fixed { self.doc.events[r].text = txt; }
        self.osi.gl_issues = self.glossary_issues();
        self.osi.gl_msg = tf!("{} lines fixed", n);
        self.run_qc();
    }

    // ------------------------------------------------------------ speech → empty lines
    /// Insert empty lines for detected speech that no existing line covers (a timer's starting point).
    pub(super) fn speech_lines(&mut self) {
        if self.media.pcm.is_empty() { self.last_status = t("Open the video or audio first (the speech is read from it)").into(); return; }
        if self.media.peaks_loading { self.last_status = t("Wait until the audio is decoded").into(); return; }
        let segs = yuzu_media::vad::speech_segments(&self.media.pcm, yuzu_media::audio::RATE);
        self.insert_speech_lines(&segs);
    }

    pub(super) fn insert_speech_lines(&mut self, segs: &[(i64, i64)]) {
        let style = self.doc.events.get(self.active).map(|e| e.style.clone()).or_else(|| self.doc.styles.first().map(|s| s.name.clone())).unwrap_or_else(|| "Default".into());
        let fresh: Vec<(i64, i64)> = segs.iter().copied().filter(|s| !self.doc.events.iter().any(|e| !e.comment && e.start < s.1 && s.0 < e.end)).collect();
        if fresh.is_empty() { self.last_status = t("No new speech found").into(); return; }
        self.push_undo();
        for (a, b) in &fresh { self.doc.events.push(Event::new(*a, *b, &style, "")); }
        self.doc.events.sort_by_key(|e| (e.start, e.end));
        self.buf_key = (usize::MAX, 0, 0);
        self.run_qc();
        self.last_status = tf!("{} speech lines added", fresh.len());
    }
}
