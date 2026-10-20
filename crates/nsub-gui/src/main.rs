//! nightsub GUI (egui). Logic lives in `state.rs`; this file only renders and routes input.

mod media_ui;
mod state;
mod worker;

use eframe::egui::{self, Align2, Color32, FontId, Key, Modifiers, RichText, Sense};
use nsub_core::{ops, tags, text, time, Color};
use nsub_lua::dialog::{DialogResponse, Val};
use state::App;
use std::collections::HashMap;
use std::path::PathBuf;
use std::time::{Duration, Instant};

#[derive(PartialEq, Clone, Copy)]
enum Side {
    Video,
    Qc,
    Styles,
}

struct Gui {
    app: App,
    side: Side,
    show_log: bool,
    show_shift: bool,
    show_replace: bool,
    shift_ms: i64,
    shift_all: bool,
    repl: ops::Replace,
    repl_msg: String,
    style_sel: usize,
    scroll_to: Option<usize>,
    last_autosave: Instant,
    last_title: String,
    dlg_vals: HashMap<String, Val>,
    dlg_open: bool,
    media: media_ui::Media,
    last_active: Option<usize>,
    show_wave: bool,
    last_doc_path: Option<PathBuf>,
}

const ROW_H: f32 = 22.0;

impl Gui {
    fn new(initial: Option<PathBuf>, ctx: egui::Context) -> Gui {
        let mut app = App::new();
        let ctx2 = ctx.clone();
        app.waker = Some(std::sync::Arc::new(move || ctx.request_repaint()));
        if let Some(p) = initial {
            app.open(&p);
        }
        // Auto-load macros from ~/.nightsub/automation/autoload/*.lua
        if let Ok(home) = std::env::var("HOME") {
            let dir = PathBuf::from(home).join(".nightsub/automation/autoload");
            if let Ok(rd) = std::fs::read_dir(dir) {
                let mut v: Vec<PathBuf> = rd.flatten().map(|e| e.path()).filter(|p| p.extension().map_or(false, |e| e == "lua")).collect();
                v.sort();
                for p in v {
                    app.add_script(p);
                }
            }
        }
        Gui {
            app,
            side: Side::Video,
            show_log: false,
            show_shift: false,
            show_replace: false,
            shift_ms: 0,
            shift_all: false,
            repl: ops::Replace { skip_tags: true, ..Default::default() },
            repl_msg: String::new(),
            style_sel: 0,
            scroll_to: None,
            last_autosave: Instant::now(),
            last_title: String::new(),
            dlg_vals: HashMap::new(),
            dlg_open: false,
            media: media_ui::Media::new(ctx2),
            last_active: None,
            show_wave: true,
            last_doc_path: None,
        }
    }

    fn open_dialog(&mut self) {
        if let Some(p) = rfd::FileDialog::new().add_filter("Subtitles", &["ass", "ssa", "srt", "vtt", "sub", "lrc"]).pick_file() {
            self.app.open(&p);
        }
    }

    fn save(&mut self, as_new: bool) {
        let path = match (&self.app.doc.path, as_new) {
            (Some(p), false) => Some(p.clone()),
            _ => rfd::FileDialog::new().add_filter("ASS", &["ass"]).add_filter("SRT", &["srt"]).add_filter("WebVTT", &["vtt"]).set_file_name("subtitle.ass").save_file(),
        };
        if let Some(p) = path {
            match self.app.doc.save_to(&p) {
                Ok(()) => self.app.status = format!("Saved {}", p.display()),
                Err(e) => self.app.status = format!("Error: {e}"),
            }
        }
    }

    fn autosave(&mut self) {
        if !self.app.doc.dirty || self.last_autosave.elapsed() < Duration::from_secs(60) {
            return;
        }
        self.last_autosave = Instant::now();
        if let Ok(home) = std::env::var("HOME") {
            let dir = PathBuf::from(home).join(".nightsub/autosave");
            if std::fs::create_dir_all(&dir).is_ok() {
                let name = self.app.doc.path.as_ref().and_then(|p| p.file_stem()).map(|s| s.to_string_lossy().to_string()).unwrap_or_else(|| "untitled".into());
                let _ = std::fs::write(dir.join(format!("{name}.autosave.ass")), self.app.doc.script.to_ass_string());
            }
        }
    }

    fn shortcuts(&mut self, ctx: &egui::Context) {
        let typing = ctx.wants_keyboard_input();
        let (mut undo, mut redo, mut save, mut open, mut find, mut dup, mut del, mut up, mut down, mut all) = (false, false, false, false, false, false, false, false, false, false);
        ctx.input_mut(|i| {
            redo = i.consume_key(Modifiers::COMMAND | Modifiers::SHIFT, Key::Z) || i.consume_key(Modifiers::COMMAND, Key::Y);
            undo = i.consume_key(Modifiers::COMMAND, Key::Z);
            save = i.consume_key(Modifiers::COMMAND, Key::S);
            open = i.consume_key(Modifiers::COMMAND, Key::O);
            find = i.consume_key(Modifiers::COMMAND, Key::F) || i.consume_key(Modifiers::COMMAND, Key::H);
            if !typing {
                dup = i.consume_key(Modifiers::COMMAND, Key::D);
                all = i.consume_key(Modifiers::COMMAND, Key::A);
                del = i.consume_key(Modifiers::NONE, Key::Delete);
                up = i.consume_key(Modifiers::NONE, Key::ArrowUp);
                down = i.consume_key(Modifiers::NONE, Key::ArrowDown);
            }
        });
        if redo { self.app.redo() } else if undo { self.app.undo() }
        if save { self.save(false) }
        if open { self.open_dialog() }
        if find { self.show_replace = true }
        if dup { self.app.duplicate_selected() }
        if all { self.app.select_all() }
        if del { self.app.delete_selected() }
        let n = self.app.doc.script.events.len();
        if (up || down) && n > 0 {
            let cur = self.app.active.unwrap_or(0);
            let next = if up { cur.saturating_sub(1) } else { (cur + 1).min(n - 1) };
            self.app.select_only(next);
            self.scroll_to = Some(next);
        }
    }

    fn attach_video(&mut self, p: PathBuf, ctx: &egui::Context) {
        self.media.load_video(p.clone(), ctx);
        if let Some(f) = self.media.fps() {
            self.app.fps = f;
        }
        if self.media.video.is_some() {
            self.app.doc.script.set_project_property("Video File", &p.to_string_lossy());
            if let Some((_, info)) = &self.media.video {
                // like Aegisub: adopt the video resolution when the script has none
                if self.app.doc.script.get_info("PlayResX").is_none() {
                    self.app.doc.script.set_info("PlayResX", &info.width.to_string());
                    self.app.doc.script.set_info("PlayResY", &info.height.to_string());
                }
            }
        }
        self.side = Side::Video;
    }

    fn autoload_media(&mut self, ctx: &egui::Context) {
        let base = self.app.doc.path.as_ref().and_then(|p| p.parent()).map(|p| p.to_path_buf());
        let resolve = |v: &str| -> Option<PathBuf> {
            let p = PathBuf::from(v);
            if p.is_file() {
                return Some(p);
            }
            let q = base.as_ref()?.join(&p);
            q.is_file().then_some(q)
        };
        if let Some(v) = self.app.doc.script.project_property("Video File").and_then(resolve) {
            self.media.load_video(v, ctx);
            if let Some(f) = self.media.fps() {
                self.app.fps = f;
            }
        }
        if self.media.audio.is_none() {
            if let Some(a) = self.app.doc.script.project_property("Audio File").and_then(resolve) {
                self.media.load_audio(a, ctx);
            }
        }
    }

    fn menu(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::top("menu").show(ctx, |ui| {
            egui::menu::bar(ui, |ui| {
                ui.menu_button("File", |ui| {
                    if ui.button("New").clicked() {
                        self.app = App::new();
                        ui.close_menu();
                    }
                    if ui.button("Open…   Ctrl+O").clicked() {
                        ui.close_menu();
                        self.open_dialog();
                    }
                    if ui.button("Save   Ctrl+S").clicked() {
                        ui.close_menu();
                        self.save(false);
                    }
                    if ui.button("Save as…").clicked() {
                        ui.close_menu();
                        self.save(true);
                    }
                    ui.separator();
                    if ui.button("Open video…").clicked() {
                        ui.close_menu();
                        if let Some(p) = rfd::FileDialog::new().add_filter("Video", &["mkv", "mp4", "avi", "webm", "mov", "m2ts", "ts", "flv"]).pick_file() {
                            self.attach_video(p, ctx);
                        }
                    }
                    if ui.button("Open audio…").clicked() {
                        ui.close_menu();
                        if let Some(p) = rfd::FileDialog::new().add_filter("Audio", &["mka", "mp3", "wav", "flac", "aac", "ogg", "opus", "m4a", "mkv", "mp4"]).pick_file() {
                            self.media.load_audio(p.clone(), ctx);
                            self.app.doc.script.set_project_property("Audio File", &p.to_string_lossy());
                        }
                    }
                });
                ui.menu_button("Edit", |ui| {
                    if ui.add_enabled(self.app.doc.can_undo(), egui::Button::new("Undo   Ctrl+Z")).clicked() {
                        self.app.undo();
                        ui.close_menu();
                    }
                    if ui.add_enabled(self.app.doc.can_redo(), egui::Button::new("Redo   Ctrl+Shift+Z")).clicked() {
                        self.app.redo();
                        ui.close_menu();
                    }
                    ui.separator();
                    if ui.button("Insert line after").clicked() { self.app.insert_after_active(); ui.close_menu(); }
                    if ui.button("Duplicate   Ctrl+D").clicked() { self.app.duplicate_selected(); ui.close_menu(); }
                    if ui.button("Merge selected").clicked() { self.app.merge_selected(); ui.close_menu(); }
                    if ui.button("Delete   Del").clicked() { self.app.delete_selected(); ui.close_menu(); }
                    ui.separator();
                    if ui.button("Find / Replace…   Ctrl+F").clicked() { self.show_replace = true; ui.close_menu(); }
                });
                ui.menu_button("Timing", |ui| {
                    if ui.button("Shift times…").clicked() { self.show_shift = true; ui.close_menu(); }
                    if ui.button("Sort by start time").clicked() { self.app.sort_by_time(); ui.close_menu(); }
                });
                ui.menu_button("Automation", |ui| {
                    if ui.button("Load Lua script…").clicked() {
                        ui.close_menu();
                        if let Some(p) = rfd::FileDialog::new().add_filter("Lua", &["lua"]).pick_file() {
                            self.app.add_script(p);
                        }
                    }
                    if ui.button("Reload scripts").clicked() { self.app.reload_scripts(); ui.close_menu(); }
                    ui.separator();
                    let macros = self.app.macros.clone();
                    let busy = self.app.is_busy();
                    if macros.is_empty() {
                        ui.label(RichText::new("(no macros loaded)").weak());
                    }
                    let mut groups: Vec<(String, Vec<&nsub_lua::MacroInfo>)> = vec![];
                    for m in &macros {
                        let g = m.name.split_once('/').map(|x| x.0.to_string()).unwrap_or_default();
                        match groups.iter_mut().find(|x| x.0 == g) {
                            Some(x) => x.1.push(m),
                            None => groups.push((g, vec![m])),
                        }
                    }
                    for (g, ms) in groups {
                        let mut draw = |ui: &mut egui::Ui| {
                            for m in &ms {
                                let label = m.name.split_once('/').map(|x| x.1).unwrap_or(&m.name);
                                if ui.add_enabled(!busy, egui::Button::new(label)).on_hover_text(&m.description).clicked() {
                                    self.app.run_macro(m.id);
                                    ui.close_menu();
                                }
                            }
                        };
                        if g.is_empty() { draw(ui) } else { ui.menu_button(g, draw); }
                    }
                });
                ui.menu_button("View", |ui| {
                    if ui.button("Video").clicked() { self.side = Side::Video; ui.close_menu(); }
                    ui.checkbox(&mut self.show_wave, "Waveform");
                    if ui.button("QC panel").clicked() { self.side = Side::Qc; ui.close_menu(); }
                    if ui.button("Styles manager").clicked() { self.side = Side::Styles; ui.close_menu(); }
                    if ui.button("Log").clicked() { self.show_log = true; ui.close_menu(); }
                });
            });
        });
    }

    fn grid(&mut self, ui: &mut egui::Ui) {
        let n = self.app.doc.script.events.len();
        let mut area = egui::ScrollArea::vertical().auto_shrink([false; 2]);
        if let Some(i) = self.scroll_to.take() {
            area = area.vertical_scroll_offset((i as f32 * ROW_H - 120.0).max(0.0));
        }
        let mods = ui.input(|i| i.modifiers);
        let mut clicked = None;
        area.show_rows(ui, ROW_H, n, |ui, range| {
            for i in range {
                let e = &self.app.doc.script.events[i];
                let (rect, resp) = ui.allocate_exact_size(egui::vec2(ui.available_width(), ROW_H), Sense::click());
                let selected = self.app.sel.contains(&i);
                let vis = ui.visuals();
                if selected {
                    ui.painter().rect_filled(rect, 0.0, vis.selection.bg_fill);
                } else if i % 2 == 1 {
                    ui.painter().rect_filled(rect, 0.0, vis.faint_bg_color);
                }
                let color = if e.comment { Color32::from_rgb(120, 160, 120) } else if self.app.bad_lines.contains(&i) { Color32::from_rgb(230, 110, 110) } else { vis.text_color() };
                let font = FontId::monospace(13.0);
                let y = rect.center().y;
                let mut x = rect.left() + 4.0;
                let cols: [(String, f32); 5] = [
                    (format!("{}", i + 1), 46.0),
                    (time::format_ass(e.start), 94.0),
                    (time::format_ass(e.end), 94.0),
                    (format!("{:.0}", text::reading_chars(&e.text) as f64 / (e.duration().max(1) as f64 / 1000.0)), 34.0),
                    (e.style.clone(), 100.0),
                ];
                for (s, w) in cols {
                    ui.painter().text(egui::pos2(x, y), Align2::LEFT_CENTER, s, font.clone(), color);
                    x += w;
                }
                let t: String = e.text.chars().take(200).collect();
                ui.painter().with_clip_rect(rect).text(egui::pos2(x, y), Align2::LEFT_CENTER, t, FontId::proportional(14.0), color);
                if resp.clicked() {
                    clicked = Some(i);
                }
            }
        });
        if let Some(i) = clicked {
            self.app.doc.end_edit_session();
            if mods.shift { self.app.select_range_to(i) } else if mods.command { self.app.toggle_select(i) } else { self.app.select_only(i) }
        }
    }

    fn editor(&mut self, ui: &mut egui::Ui) {
        let Some(a) = self.app.active.filter(|&a| a < self.app.doc.script.events.len()) else {
            ui.label(RichText::new("No line selected").weak());
            return;
        };
        let orig = self.app.doc.script.events[a].clone();
        let mut ev = orig.clone();
        let styles: Vec<String> = self.app.doc.script.styles.iter().map(|s| s.name.clone()).collect();
        ui.horizontal_wrapped(|ui| {
            ui.checkbox(&mut ev.comment, "Comment");
            ui.label("Layer");
            ui.add(egui::DragValue::new(&mut ev.layer).range(0..=999));
            ui.label("Start");
            time_drag(ui, &mut ev.start);
            ui.label("End");
            time_drag(ui, &mut ev.end);
            ui.label("Dur");
            let mut d = ev.end - ev.start;
            if ui.add(egui::DragValue::new(&mut d).speed(10.0).suffix(" ms")).changed() {
                ev.end = ev.start + d.max(0);
            }
            ui.label("Style");
            egui::ComboBox::from_id_salt("style").selected_text(ev.style.clone()).show_ui(ui, |ui| {
                for s in &styles {
                    ui.selectable_value(&mut ev.style, s.clone(), s);
                }
            });
            ui.label("Actor");
            ui.add(egui::TextEdit::singleline(&mut ev.actor).desired_width(80.0));
            ui.label("Effect");
            ui.add(egui::TextEdit::singleline(&mut ev.effect).desired_width(80.0));
            ui.label("L/R/V");
            ui.add(egui::DragValue::new(&mut ev.margin_l));
            ui.add(egui::DragValue::new(&mut ev.margin_r));
            ui.add(egui::DragValue::new(&mut ev.margin_t));
        });
        let mut out = egui::TextEdit::singleline(&mut ev.text).font(FontId::proportional(16.0)).desired_width(f32::INFINITY).show(ui);
        // Shift+Enter inserts the ASS line break \N at the cursor.
        let shift_enter = ui.input(|i| i.key_pressed(Key::Enter) && i.modifiers.shift);
        if shift_enter && (out.response.has_focus() || out.response.lost_focus()) {
            let idx = out.cursor_range.map(|c| c.primary.ccursor.index).unwrap_or(ev.text.chars().count());
            let byte = ev.text.char_indices().nth(idx).map(|x| x.0).unwrap_or(ev.text.len());
            ev.text.insert_str(byte, "\\N");
            let c = egui::text::CCursor::new(idx + 2);
            out.state.cursor.set_char_range(Some(egui::text::CCursorRange::one(c)));
            out.state.store(ui.ctx(), out.response.id);
            out.response.request_focus();
        }
        let vis = tags::visible_text(&ev.text);
        ui.horizontal(|ui| {
            let dur = (ev.end - ev.start).max(1) as f64 / 1000.0;
            ui.label(RichText::new(format!("{} chars · {:.1} CPS · longest line {}", text::reading_chars(&ev.text), text::reading_chars(&ev.text) as f64 / dur, text::longest_line(&ev.text))).weak());
            ui.label(RichText::new(vis.replace('\n', "  ⏎  ")).italics());
        });
        if ev != orig {
            if ev.text != orig.text { self.app.doc.touch(&format!("{a}:text")) } else { self.app.doc.touch(&format!("{a}:meta")) }
            self.app.doc.script.events[a] = ev;
            self.app.refresh_qc();
        }
    }

    fn qc_panel(&mut self, ui: &mut egui::Ui) {
        let mut changed = false;
        let c = &mut self.app.qc_cfg;
        egui::Grid::new("qccfg").num_columns(2).show(ui, |ui| {
            ui.label("Max CPS");
            changed |= ui.add(egui::DragValue::new(&mut c.max_cps).speed(0.5)).changed();
            ui.end_row();
            ui.label("Min duration (ms)");
            changed |= ui.add(egui::DragValue::new(&mut c.min_duration_ms).speed(10.0)).changed();
            ui.end_row();
            ui.label("Max duration (ms)");
            changed |= ui.add(egui::DragValue::new(&mut c.max_duration_ms).speed(10.0)).changed();
            ui.end_row();
            ui.label("Max line length");
            changed |= ui.add(egui::DragValue::new(&mut c.max_line_len)).changed();
            ui.end_row();
            ui.label("Max lines");
            changed |= ui.add(egui::DragValue::new(&mut c.max_lines)).changed();
            ui.end_row();
            ui.label("Min gap (ms)");
            changed |= ui.add(egui::DragValue::new(&mut c.min_gap_ms).speed(5.0)).changed();
            ui.end_row();
        });
        if changed {
            self.app.refresh_qc();
        }
        ui.horizontal(|ui| {
            if ui.button("Re-check").clicked() { self.app.refresh_qc(); }
            if ui.button("Fix overlaps / short / spaces").clicked() {
                let n = self.app.fix_all();
                self.app.status = format!("{n} fix(es) applied");
            }
        });
        ui.label(format!("{} issue(s)", self.app.issues.len()));
        let mut jump = None;
        egui::ScrollArea::vertical().show(ui, |ui| {
            for iss in &self.app.issues {
                if ui.selectable_label(false, format!("#{} {:?}: {}", iss.index + 1, iss.kind, iss.message)).clicked() {
                    jump = Some(iss.index);
                }
            }
        });
        if let Some(i) = jump {
            self.app.select_only(i);
            self.scroll_to = Some(i);
        }
    }

    fn styles_panel(&mut self, ui: &mut egui::Ui) {
        let n = self.app.doc.script.styles.len();
        self.style_sel = self.style_sel.min(n.saturating_sub(1));
        ui.horizontal(|ui| {
            if ui.button("New").clicked() {
                self.app.doc.checkpoint();
                let mut s = nsub_core::Style::default();
                s.name = format!("Style{}", n + 1);
                self.app.doc.script.styles.push(s);
                self.style_sel = n;
            }
            if ui.button("Copy").clicked() && n > 0 {
                self.app.doc.checkpoint();
                let mut s = self.app.doc.script.styles[self.style_sel].clone();
                s.name.push_str(" copy");
                self.app.doc.script.styles.push(s);
                self.style_sel = n;
            }
            if ui.add_enabled(n > 1, egui::Button::new("Delete")).clicked() {
                self.app.doc.checkpoint();
                self.app.doc.script.styles.remove(self.style_sel);
                self.style_sel = 0;
            }
        });
        egui::ComboBox::from_id_salt("stylesel").selected_text(self.app.doc.script.styles.get(self.style_sel).map(|s| s.name.clone()).unwrap_or_default()).show_ui(ui, |ui| {
            for (i, s) in self.app.doc.script.styles.iter().enumerate() {
                ui.selectable_value(&mut self.style_sel, i, &s.name);
            }
        });
        let Some(orig) = self.app.doc.script.styles.get(self.style_sel).cloned() else { return };
        let mut s = orig.clone();
        egui::ScrollArea::vertical().show(ui, |ui| {
            egui::Grid::new("styleg").num_columns(2).show(ui, |ui| {
                ui.label("Name");
                ui.text_edit_singleline(&mut s.name);
                ui.end_row();
                ui.label("Font");
                ui.text_edit_singleline(&mut s.fontname);
                ui.end_row();
                ui.label("Size");
                ui.add(egui::DragValue::new(&mut s.fontsize).speed(0.5));
                ui.end_row();
                for (label, col) in [("Primary", &mut s.color1), ("Secondary", &mut s.color2), ("Outline", &mut s.color3), ("Shadow", &mut s.color4)] {
                    ui.label(label);
                    let mut c = Color32::from_rgba_unmultiplied(col.r, col.g, col.b, 255 - col.a);
                    if ui.color_edit_button_srgba(&mut c).changed() {
                        *col = Color::new(c.r(), c.g(), c.b(), 255 - c.a());
                    }
                    ui.end_row();
                }
                ui.label("Style");
                ui.horizontal(|ui| {
                    ui.checkbox(&mut s.bold, "B");
                    ui.checkbox(&mut s.italic, "I");
                    ui.checkbox(&mut s.underline, "U");
                    ui.checkbox(&mut s.strikeout, "S");
                });
                ui.end_row();
                ui.label("Scale X/Y");
                ui.horizontal(|ui| {
                    ui.add(egui::DragValue::new(&mut s.scale_x));
                    ui.add(egui::DragValue::new(&mut s.scale_y));
                });
                ui.end_row();
                ui.label("Spacing / Angle");
                ui.horizontal(|ui| {
                    ui.add(egui::DragValue::new(&mut s.spacing).speed(0.1));
                    ui.add(egui::DragValue::new(&mut s.angle));
                });
                ui.end_row();
                ui.label("Border style");
                ui.add(egui::DragValue::new(&mut s.borderstyle).range(1..=4));
                ui.end_row();
                ui.label("Outline / Shadow");
                ui.horizontal(|ui| {
                    ui.add(egui::DragValue::new(&mut s.outline).speed(0.1));
                    ui.add(egui::DragValue::new(&mut s.shadow).speed(0.1));
                });
                ui.end_row();
                ui.label("Alignment");
                egui::Grid::new("align").show(ui, |ui| {
                    for row in [[7, 8, 9], [4, 5, 6], [1, 2, 3]] {
                        for a in row {
                            ui.selectable_value(&mut s.align, a, a.to_string());
                        }
                        ui.end_row();
                    }
                });
                ui.end_row();
                ui.label("Margins L/R/V");
                ui.horizontal(|ui| {
                    ui.add(egui::DragValue::new(&mut s.margin_l));
                    ui.add(egui::DragValue::new(&mut s.margin_r));
                    ui.add(egui::DragValue::new(&mut s.margin_t));
                });
                ui.end_row();
            });
        });
        s.margin_b = s.margin_t;
        if s != orig {
            self.app.doc.touch(&format!("style{}", self.style_sel));
            if s.name != orig.name {
                for e in &mut self.app.doc.script.events {
                    if e.style == orig.name {
                        e.style = s.name.clone();
                    }
                }
            }
            self.app.doc.script.styles[self.style_sel] = s;
        }
    }

    fn dialog_window(&mut self, ctx: &egui::Context) {
        let Some(req) = self.app.pending_dialog.clone() else {
            self.dlg_open = false;
            return;
        };
        if !self.dlg_open {
            self.dlg_vals = req.controls.iter().filter(|c| !c.name.is_empty()).map(|c| (c.name.clone(), c.value.clone())).collect();
            self.dlg_open = true;
        }
        let title = self.app.running.as_ref().map(|r| r.name.clone()).unwrap_or_else(|| "Automation".into());
        let mut answer: Option<Option<String>> = None;
        egui::Window::new(title).collapsible(false).resizable(false).anchor(Align2::CENTER_CENTER, [0.0, 0.0]).show(ctx, |ui| {
            let mut rows: Vec<i32> = req.controls.iter().map(|c| c.y).collect();
            rows.sort_unstable();
            rows.dedup();
            for y in rows {
                let mut cs: Vec<&nsub_lua::dialog::Control> = req.controls.iter().filter(|c| c.y == y).collect();
                cs.sort_by_key(|c| c.x);
                ui.horizontal(|ui| {
                    for c in cs {
                        control_ui(ui, c, &mut self.dlg_vals);
                    }
                });
            }
            ui.separator();
            ui.horizontal(|ui| {
                if req.buttons.is_empty() {
                    if ui.button("OK").clicked() { answer = Some(Some("OK".into())) }
                    if ui.button("Cancel").clicked() { answer = Some(None) }
                } else {
                    for b in &req.buttons {
                        if ui.button(b).clicked() {
                            let is_cancel = req.cancel_id.as_deref() == Some(b.as_str());
                            answer = Some(if is_cancel { None } else { Some(b.clone()) });
                        }
                    }
                }
            });
        });
        if let Some(button) = answer {
            self.dlg_open = false;
            let values = std::mem::take(&mut self.dlg_vals);
            self.app.answer_dialog(DialogResponse { button, values });
        }
    }

    fn windows(&mut self, ctx: &egui::Context) {
        let mut open = self.show_shift;
        egui::Window::new("Shift times").open(&mut open).collapsible(false).show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label("Milliseconds");
                ui.add(egui::DragValue::new(&mut self.shift_ms).speed(10.0));
            });
            ui.checkbox(&mut self.shift_all, "All lines (otherwise selected)");
            if ui.button("Apply").clicked() {
                if self.shift_all { self.app.shift_all(self.shift_ms) } else { self.app.shift_selected(self.shift_ms) }
                self.app.refresh_qc();
            }
        });
        self.show_shift = open;

        let mut open = self.show_replace;
        egui::Window::new("Find / Replace").open(&mut open).collapsible(false).show(ctx, |ui| {
            egui::Grid::new("repl").num_columns(2).show(ui, |ui| {
                ui.label("Find");
                ui.text_edit_singleline(&mut self.repl.pattern);
                ui.end_row();
                ui.label("Replace");
                ui.text_edit_singleline(&mut self.repl.with);
                ui.end_row();
            });
            ui.horizontal(|ui| {
                ui.checkbox(&mut self.repl.regex, "Regex");
                ui.checkbox(&mut self.repl.case_sensitive, "Match case");
                ui.checkbox(&mut self.repl.whole_word, "Whole word");
                ui.checkbox(&mut self.repl.skip_tags, "Skip {tags}");
            });
            ui.horizontal(|ui| {
                if ui.button("Replace all").clicked() {
                    self.app.doc.checkpoint();
                    match ops::find_replace(&mut self.app.doc.script, &self.repl) {
                        Ok(n) => self.repl_msg = format!("{n} line(s) changed"),
                        Err(e) => self.repl_msg = e,
                    }
                    self.app.refresh_qc();
                }
                if ui.button("Find next").clicked() {
                    let start = self.app.active.map_or(0, |a| a + 1);
                    match find_next(&self.app, &self.repl, start) {
                        Ok(Some(i)) => {
                            self.app.select_only(i);
                            self.scroll_to = Some(i);
                            self.repl_msg = format!("line {}", i + 1);
                        }
                        Ok(None) => self.repl_msg = "not found".into(),
                        Err(e) => self.repl_msg = e,
                    }
                }
            });
            ui.label(&self.repl_msg);
        });
        self.show_replace = open;

        let mut open = self.show_log;
        egui::Window::new("Log").open(&mut open).default_size([520.0, 300.0]).show(ctx, |ui| {
            egui::ScrollArea::vertical().show(ui, |ui| {
                ui.add(egui::TextEdit::multiline(&mut self.app.log.as_str()).font(FontId::monospace(12.0)).desired_width(f32::INFINITY));
            });
        });
        self.show_log = open;
    }
}

fn find_next(app: &App, r: &ops::Replace, start: usize) -> Result<Option<usize>, String> {
    let mut pat = if r.regex { r.pattern.clone() } else { regex::escape(&r.pattern) };
    if r.whole_word {
        pat = format!(r"\b(?:{pat})\b");
    }
    if !r.case_sensitive {
        pat = format!("(?i){pat}");
    }
    let re = regex::Regex::new(&pat).map_err(|e| e.to_string())?;
    let n = app.doc.script.events.len();
    Ok((0..n).map(|k| (start + k) % n.max(1)).find(|&i| re.is_match(&app.doc.script.events[i].text)))
}

fn time_drag(ui: &mut egui::Ui, ms: &mut i64) {
    let mut v = *ms as f64;
    let r = ui.add(
        egui::DragValue::new(&mut v)
            .speed(10.0)
            .custom_formatter(|n, _| time::format_ass(n as i64))
            .custom_parser(|s| time::parse_ass(s).map(|x| x as f64)),
    );
    if r.changed() {
        *ms = (v.round() as i64).max(0);
    }
}

impl eframe::App for Gui {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        for f in ctx.input(|i| i.raw.dropped_files.clone()) {
            if let Some(p) = f.path {
                if p.extension().map_or(false, |e| e == "lua") {
                    self.app.add_script(p);
                } else {
                    self.app.open(&p);
                }
            }
        }
        self.app.pump();
        self.app.tick_qc();
        if self.app.qc_busy() {
            ctx.request_repaint_after(Duration::from_millis(150));
        }
        if self.last_active != self.app.active {
            self.last_active = self.app.active;
            self.media.on_active_changed(&self.app);
        }
        if self.app.doc.path != self.last_doc_path {
            self.last_doc_path = self.app.doc.path.clone();
            self.autoload_media(ctx);
        }
        self.shortcuts(ctx);
        self.autosave();
        let title = self.app.doc.title();
        if title != self.last_title {
            ctx.send_viewport_cmd(egui::ViewportCommand::Title(title.clone()));
            self.last_title = title;
        }
        self.menu(ctx);
        egui::TopBottomPanel::bottom("status").show(ctx, |ui| {
            ui.horizontal(|ui| {
                if let Some(r) = self.app.running.clone() {
                    ui.add(egui::ProgressBar::new(r.fraction as f32).desired_width(160.0).animate(true));
                    ui.label(format!("{} {}", r.name, r.task));
                    if ui.small_button("Cancel").clicked() {
                        self.app.cancel_macro();
                    }
                } else {
                    ui.label(&self.app.status);
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(format!("{} lines · {} selected · {} issues", self.app.doc.script.events.len(), self.app.sel.len(), self.app.issues.len()));
                });
            });
        });
        if self.show_wave {
            egui::TopBottomPanel::bottom("wave").resizable(true).default_height(110.0).height_range(60.0..=400.0).show(ctx, |ui| self.media.waveform(ui, &mut self.app));
        }
        egui::TopBottomPanel::bottom("editor").min_height(90.0).show(ctx, |ui| self.editor(ui));
        egui::SidePanel::right("side").default_width(460.0).width_range(300.0..=1000.0).show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.selectable_value(&mut self.side, Side::Video, "Video");
                ui.selectable_value(&mut self.side, Side::Qc, "Quality");
                ui.selectable_value(&mut self.side, Side::Styles, "Styles");
            });
            ui.separator();
            match self.side {
                Side::Video => self.media.video_panel(ui, &mut self.app),
                Side::Qc => self.qc_panel(ui),
                Side::Styles => self.styles_panel(ui),
            }
        });
        egui::CentralPanel::default().show(ctx, |ui| self.grid(ui));
        self.windows(ctx);
        self.dialog_window(ctx);
    }
}

fn main() -> eframe::Result {
    let initial = std::env::args().nth(1).map(PathBuf::from);
    let options = eframe::NativeOptions { viewport: egui::ViewportBuilder::default().with_inner_size([1280.0, 780.0]).with_title("nightsub"), ..Default::default() };
    eframe::run_native("nightsub", options, Box::new(|cc| Ok(Box::new(Gui::new(initial, cc.egui_ctx.clone())))))
}

fn control_ui(ui: &mut egui::Ui, c: &nsub_lua::dialog::Control, vals: &mut HashMap<String, Val>) {
    let entry = vals.entry(c.name.clone()).or_insert_with(|| c.value.clone());
    match c.class.as_str() {
        "label" => {
            ui.label(&c.label);
        }
        "edit" | "textbox" | "alpha" => {
            let mut t = match entry { Val::Str(s) => s.clone(), Val::Num(n) => n.to_string(), _ => String::new() };
            let w = (c.width.max(1) as f32 * 120.0).min(520.0);
            let r = if c.class == "textbox" {
                ui.add(egui::TextEdit::multiline(&mut t).desired_width(w).desired_rows(c.height.max(2) as usize))
            } else {
                ui.add(egui::TextEdit::singleline(&mut t).desired_width(w))
            };
            if r.changed() { *entry = Val::Str(t); }
            if !c.hint.is_empty() { r.on_hover_text(&c.hint); }
        }
        "intedit" | "floatedit" => {
            let mut v = match entry { Val::Num(n) => *n, Val::Str(s) => s.parse().unwrap_or(0.0), _ => 0.0 };
            let mut d = egui::DragValue::new(&mut v);
            if c.class == "intedit" { d = d.speed(1.0).max_decimals(0) } else { d = d.speed(c.step.unwrap_or(0.1)) }
            if let (Some(lo), Some(hi)) = (c.min, c.max) { d = d.range(lo..=hi) }
            if ui.add(d).changed() { *entry = Val::Num(v); }
        }
        "checkbox" => {
            let mut b = matches!(entry, Val::Bool(true));
            if ui.checkbox(&mut b, &c.label).changed() { *entry = Val::Bool(b); }
        }
        "dropdown" => {
            let mut cur = match entry { Val::Str(s) => s.clone(), _ => String::new() };
            egui::ComboBox::from_id_salt(&c.name).selected_text(cur.clone()).show_ui(ui, |ui| {
                for it in &c.items { ui.selectable_value(&mut cur, it.clone(), it); }
            });
            *entry = Val::Str(cur);
        }
        "color" | "coloralpha" => {
            let col = match entry { Val::Str(s) => Color::parse(s).unwrap_or(Color::WHITE), _ => Color::WHITE };
            let mut c32 = Color32::from_rgba_unmultiplied(col.r, col.g, col.b, 255 - col.a);
            if ui.color_edit_button_srgba(&mut c32).changed() {
                let n = Color::new(c32.r(), c32.g(), c32.b(), 255 - c32.a());
                *entry = Val::Str(if c.class == "color" { n.to_tag_string() } else { n.to_style_string() });
            }
        }
        _ => {
            ui.label(format!("[{}]", c.class));
        }
    }
}
