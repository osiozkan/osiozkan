//! Preferences window: language, theme, size, limits, video/audio defaults, script folders and
//! the shortcut editor (record a key, second binding, reset, conflicts block saving).

use crate::commands::{self, Keymap, COMMANDS};
use crate::i18n::{t, tf, Lang};
use crate::settings::Settings;
use eframe::egui::{self, Color32, Key, RichText};

pub struct Prefs {
    pub tab: usize,
    pub s: Settings,
    pub keymap: Keymap,
    pub search: String,
    /// (command id, binding slot) waiting for a key press.
    pub recording: Option<(&'static str, usize)>,
}

#[derive(PartialEq)]
pub enum Action { None, Save, Close }

impl Prefs {
    pub fn new(s: &Settings, keymap: &Keymap) -> Prefs {
        Prefs { tab: 0, s: s.clone(), keymap: keymap.clone(), search: String::new(), recording: None }
    }
}

/// Take the next key press for the shortcut being recorded.
fn record(ctx: &egui::Context, p: &mut Prefs) {
    let Some((id, slot)) = p.recording else { return };
    let got = ctx.input_mut(|i| {
        let mut res = None;
        i.events.retain(|e| {
            if res.is_some() { return true; }
            if let egui::Event::Key { key, pressed: true, modifiers, .. } = e {
                res = Some((*key, *modifiers));
                return false;
            }
            true
        });
        res
    });
    let Some((key, m)) = got else { return };
    p.recording = None;
    let mut keys = p.keymap.get(id).to_vec();
    match key {
        Key::Escape => {}
        Key::Backspace if m.is_none() => { if slot < keys.len() { keys.remove(slot); } p.keymap.set(id, keys); }
        _ => {
            let mods = egui::Modifiers { ctrl: false, mac_cmd: false, command: m.command || m.ctrl || m.mac_cmd, alt: m.alt, shift: m.shift };
            let sc = egui::KeyboardShortcut::new(mods, key);
            if slot < keys.len() { keys[slot] = sc; } else { keys.push(sc); }
            p.keymap.set(id, keys);
        }
    }
}

pub fn show(ctx: &egui::Context, p: &mut Prefs) -> Action {
    record(ctx, p);
    let mut action = Action::None;
    let mut open = true;
    egui::Window::new(t("Preferences")).open(&mut open).default_width(720.0).default_height(540.0).collapsible(false).show(ctx, |ui| {
        ui.horizontal(|ui| {
            for (i, name) in [t("General"), t("Video & audio"), t("Shortcuts"), t("Script folders")].iter().enumerate() {
                ui.selectable_value(&mut p.tab, i, *name);
            }
        });
        ui.separator();
        match p.tab {
            0 => general(ui, &mut p.s),
            1 => media(ui, &mut p.s),
            2 => shortcuts(ui, p),
            _ => folders(ui, &mut p.s),
        }
        ui.separator();
        let conflicts = p.keymap.conflicts();
        for (sc, a, b) in conflicts.iter().take(4) {
            let name = |id: &str| commands::find(id).map(|c| t(c.label)).unwrap_or("?");
            ui.colored_label(Color32::from_rgb(255, 120, 100), tf!("Conflict: {} is also used by {}", format!("{} ({})", sc, name(a)), name(b)));
        }
        ui.horizontal(|ui| {
            if ui.add_enabled(conflicts.is_empty(), egui::Button::new(t("Save"))).on_disabled_hover_text(t("Resolve the conflicts before saving.")).clicked() { action = Action::Save; }
            if ui.button(t("Close")).clicked() { action = Action::Close; }
        });
    });
    if !open { action = Action::Close; }
    action
}

fn general(ui: &mut egui::Ui, s: &mut Settings) {
    egui::Grid::new("prefs_general").num_columns(2).spacing([16.0, 8.0]).show(ui, |ui| {
        ui.label(t("Interface language"));
        ui.horizontal(|ui| {
            for l in [Lang::Tr, Lang::En] { if ui.radio(s.lang == l.code(), l.name()).clicked() { s.lang = l.code().into(); } }
        });
        ui.end_row();
        ui.label(t("Theme"));
        ui.horizontal(|ui| { ui.radio_value(&mut s.dark, true, t("Dark")); ui.radio_value(&mut s.dark, false, t("Light")); });
        ui.end_row();
        ui.label(t("Interface size"));
        ui.add(egui::Slider::new(&mut s.ui_scale, 0.75..=2.0).step_by(0.05).suffix("×"));
        ui.end_row();
        ui.label(t("Autosave every (s, 0 = off)"));
        ui.add(egui::DragValue::new(&mut s.autosave_secs).range(0..=3600));
        ui.end_row();
        ui.label(t("Max characters per second"));
        ui.add(egui::DragValue::new(&mut s.max_cps).range(5.0..=60.0).speed(0.5));
        ui.end_row();
        ui.label(t("Max characters per line"));
        ui.add(egui::DragValue::new(&mut s.max_cpl).range(10..=200));
        ui.end_row();
        ui.label(t("Default frame rate"));
        egui::ComboBox::from_id_salt("prefs_fps").selected_text(format!("{:.3}", s.fps.0 as f64 / s.fps.1 as f64)).show_ui(ui, |ui| {
            for (n, d, l) in [(24000, 1001, "23.976"), (24, 1, "24"), (25, 1, "25"), (30000, 1001, "29.970"), (30, 1, "30"), (60000, 1001, "59.940"), (60, 1, "60")] {
                ui.selectable_value(&mut s.fps, (n, d), l);
            }
        });
        ui.end_row();
    });
}

fn media(ui: &mut egui::Ui, s: &mut Settings) {
    egui::Grid::new("prefs_media").num_columns(2).spacing([16.0, 8.0]).show(ui, |ui| {
        ui.label(t("Volume"));
        ui.add(egui::Slider::new(&mut s.volume, 0.0..=150.0).suffix(" %"));
        ui.end_row();
        ui.label("");
        ui.checkbox(&mut s.snap_keyframes, t("Snap timing edits to keyframes"));
        ui.end_row();
        ui.label("");
        ui.checkbox(&mut s.follow, t("Video follows the active line"));
        ui.end_row();
        ui.label("");
        ui.checkbox(&mut s.show_spectrum, t("Show the spectrum by default"));
        ui.end_row();
        ui.label(t("Karaoke tap latency (ms)"));
        ui.add(egui::DragValue::new(&mut s.tap_latency).range(0..=400));
        ui.end_row();
    });
}

fn shortcuts(ui: &mut egui::Ui, p: &mut Prefs) {
    ui.horizontal(|ui| {
        ui.label(t("Search"));
        ui.text_edit_singleline(&mut p.search);
        if ui.button(t("Reset all")).clicked() { p.keymap = Keymap::default(); }
    });
    ui.weak(t("Single keys and arrows only work while no text box is being edited."));
    let q = commands::fold(&p.search);
    egui::ScrollArea::vertical().max_height(360.0).show(ui, |ui| {
        egui::Grid::new("prefs_keys").num_columns(5).striped(true).spacing([12.0, 4.0]).show(ui, |ui| {
            ui.strong(t("Command")); ui.strong(t("Shortcut")); ui.strong(t("Second")); ui.label(""); ui.label(""); ui.end_row();
            for c in COMMANDS {
                let label = format!("{} › {}", t(c.group), t(c.label));
                if !q.is_empty() && commands::score(&q, &commands::fold(&label)).is_none() { continue; }
                ui.label(label);
                let keys = p.keymap.get(c.id).to_vec();
                for slot in 0..2 {
                    let rec = p.recording == Some((c.id, slot));
                    let txt = if rec { RichText::new(t("press keys…")).color(Color32::from_rgb(255, 200, 80)) } else { RichText::new(keys.get(slot).map(commands::display).unwrap_or_else(|| "—".into())).monospace() };
                    if ui.add(egui::Button::new(txt).min_size(egui::vec2(120.0, 0.0))).on_hover_text(t("Esc cancels, Backspace clears")).clicked() {
                        p.recording = Some((c.id, slot.min(keys.len())));
                    }
                }
                let def: Vec<String> = c.keys.to_vec().iter().map(|k| k.to_string()).collect();
                let cur: Vec<String> = keys.iter().map(commands::to_text).collect();
                if ui.add_enabled(def != cur, egui::Button::new(t("Reset")).small()).clicked() {
                    p.keymap.set(c.id, c.keys.iter().filter_map(|k| commands::parse(k)).collect());
                }
                ui.label("");
                ui.end_row();
            }
        });
    });
}

fn folders(ui: &mut egui::Ui, s: &mut Settings) {
    let mut remove = None;
    for (i, d) in s.script_dirs.iter().enumerate() {
        ui.horizontal(|ui| {
            ui.label(RichText::new(d.display().to_string()).monospace());
            if ui.small_button(t("Remove")).clicked() { remove = Some(i); }
        });
    }
    if let Some(i) = remove { s.script_dirs.remove(i); }
    if ui.button(t("Add…")).clicked() {
        if let Some(d) = rfd::FileDialog::new().pick_folder() { s.script_dirs.push(d); }
    }
    ui.add_space(8.0);
    ui.checkbox(&mut s.auto_reload, t("Reload scripts when they change on disk"));
}
