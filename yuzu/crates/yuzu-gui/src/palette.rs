//! Command palette (Ctrl+Shift+P): every command and Automation macro behind one search box.
//! Matching ignores case and Turkish letters ("kirma" finds "kırma"); recently used commands
//! come first; ":123" jumps to line 123.

use crate::commands::{fold, score};
use crate::i18n::{t, tf};
use eframe::egui::{self, Color32, Key};

#[derive(Clone, Debug, PartialEq)]
pub enum Pick {
    Command(&'static str),
    Macro { script: usize, macro_idx: usize, name: String },
    Line(usize),
}

pub struct Entry {
    pub label: String,
    pub group: String,
    pub shortcut: String,
    pub pick: Pick,
}

#[derive(Default)]
pub struct Palette {
    pub query: String,
    pub sel: usize,
    focus_pending: bool,
}

impl Palette {
    pub fn new() -> Palette { Palette { focus_pending: true, ..Default::default() } }
}

fn recent_key(p: &Pick) -> Option<String> {
    match p { Pick::Command(id) => Some(id.to_string()), Pick::Macro { name, .. } => Some(format!("macro:{}", name)), Pick::Line(_) => None }
}

/// Entries matching the query, best first.
pub fn filter<'a>(query: &str, entries: &'a [Entry], recent: &[String], n_lines: usize) -> Vec<(Option<&'a Entry>, Pick)> {
    let q = query.trim();
    if let Some(n) = q.strip_prefix(':').and_then(|r| r.trim().parse::<usize>().ok()) {
        return vec![(None, Pick::Line(n.clamp(1, n_lines.max(1)) - 1))];
    }
    let fq = fold(q);
    let mut scored: Vec<(f32, &Entry)> = entries.iter().filter_map(|e| {
        let hay = fold(&format!("{} {}", e.group, e.label));
        let mut s = score(&fq, &hay)?;
        if let Some(k) = recent_key(&e.pick) {
            if let Some(i) = recent.iter().position(|r| *r == k) { s += (recent.len() - i) as f32 * if q.is_empty() { 100.0 } else { 3.0 }; }
        }
        Some((s, e))
    }).collect();
    scored.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
    scored.into_iter().take(80).map(|(_, e)| (Some(e), e.pick.clone())).collect()
}

/// Draw the palette. Returns Some(choice) when it should close (None inside = dismissed).
pub fn show(ctx: &egui::Context, st: &mut Palette, entries: &[Entry], recent: &[String], n_lines: usize) -> Option<Option<Pick>> {
    let hits = filter(&st.query, entries, recent, n_lines);
    let (up, down, enter, esc) = ctx.input_mut(|i| (
        i.consume_key(egui::Modifiers::NONE, Key::ArrowUp),
        i.consume_key(egui::Modifiers::NONE, Key::ArrowDown),
        i.consume_key(egui::Modifiers::NONE, Key::Enter),
        i.consume_key(egui::Modifiers::NONE, Key::Escape),
    ));
    if esc { return Some(None); }
    if down { st.sel = (st.sel + 1).min(hits.len().saturating_sub(1)); }
    if up { st.sel = st.sel.saturating_sub(1); }
    st.sel = st.sel.min(hits.len().saturating_sub(1));
    if enter { return Some(hits.get(st.sel).map(|h| h.1.clone())); }
    let mut result = None;
    let width = (ctx.content_rect().width() * 0.5).clamp(420.0, 680.0);
    egui::Window::new("command_palette").title_bar(false).resizable(false).collapsible(false)
        .anchor(egui::Align2::CENTER_TOP, [0.0, 56.0]).min_width(width).max_width(width)
        .show(ctx, |ui| {
            let r = ui.add(egui::TextEdit::singleline(&mut st.query).hint_text(t("Type a command…  (:123 goes to line 123)")).desired_width(f32::INFINITY));
            if st.focus_pending { r.request_focus(); st.focus_pending = false; }
            if r.changed() { st.sel = 0; }
            ui.separator();
            egui::ScrollArea::vertical().max_height(420.0).auto_shrink([false, true]).show(ui, |ui| {
                if hits.is_empty() { ui.weak(t("No matching command")); }
                for (k, (e, pick)) in hits.iter().enumerate() {
                    let selected = k == st.sel;
                    let (label, group, sc) = match e {
                        Some(e) => (e.label.clone(), e.group.clone(), e.shortcut.clone()),
                        None => (match pick { Pick::Line(n) => tf!("Go to line {}", n + 1), _ => String::new() }, String::new(), String::new()),
                    };
                    // one full-width row: label, group (grey), shortcut on the right
                    let (rect, resp) = ui.allocate_exact_size(egui::vec2(ui.available_width(), 24.0), egui::Sense::click());
                    let v = ui.visuals();
                    if selected { ui.painter().rect_filled(rect, 3.0, v.selection.bg_fill); } else if resp.hovered() { ui.painter().rect_filled(rect, 3.0, v.widgets.hovered.weak_bg_fill); }
                    let fg = if selected { v.selection.stroke.color } else { v.text_color() };
                    let font = egui::FontId::proportional(14.0);
                    let g = ui.painter().layout_no_wrap(label, font, fg);
                    let lw = g.size().x;
                    ui.painter().galley(rect.left_center() + egui::vec2(8.0, -g.size().y / 2.0), g, fg);
                    if !group.is_empty() {
                        ui.painter().text(rect.left_center() + egui::vec2(18.0 + lw, 0.0), egui::Align2::LEFT_CENTER, group, egui::FontId::proportional(11.5), Color32::GRAY);
                    }
                    if !sc.is_empty() {
                        ui.painter().text(rect.right_center() - egui::vec2(8.0, 0.0), egui::Align2::RIGHT_CENTER, sc, egui::FontId::monospace(12.0), Color32::GRAY);
                    }
                    if selected && (up || down) { resp.scroll_to_me(None); }
                    if resp.clicked() { result = Some(Some(pick.clone())); }
                }
            });
        });
    result
}

/// Remember `pick` at the front of the recent list (max 20).
pub fn remember(recent: &mut Vec<String>, pick: &Pick) {
    if let Some(k) = recent_key(pick) {
        recent.retain(|r| *r != k);
        recent.insert(0, k);
        recent.truncate(20);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entries() -> Vec<Entry> {
        let mk = |l: &str, g: &str, id: &'static str| Entry { label: l.into(), group: g.into(), shortcut: String::new(), pick: Pick::Command(id) };
        vec![mk("Yazı tipi toplayıcı / gömme…", "Araçlar", "fonts"), mk("Satır kırmalarını dengele", "Araçlar", "fix"), mk("Denetim (QC)…", "Araçlar", "qc"), mk("Kaydet", "Dosya", "save")]
    }

    #[test]
    fn search_line_jump_and_recent() {
        let e = entries();
        assert_eq!(filter("yazi tipi", &e, &[], 10)[0].1, Pick::Command("fonts"));
        assert_eq!(filter("kirma", &e, &[], 10)[0].1, Pick::Command("fix"));
        assert_eq!(filter(":7", &e, &[], 10)[0].1, Pick::Line(6));
        assert_eq!(filter(":70", &e, &[], 10)[0].1, Pick::Line(9));
        // empty query: recent first
        let mut recent = vec![];
        remember(&mut recent, &Pick::Command("qc"));
        assert_eq!(filter("", &e, &recent, 10)[0].1, Pick::Command("qc"));
    }
}
