//! Syntax colouring for ASS dialogue text: override blocks dim/colourful, line breaks accented.

use egui::text::{LayoutJob, TextFormat};
use egui::{Color32, FontId};

pub fn layout(text: &str, font: FontId, base: Color32, wrap_width: f32) -> LayoutJob {
    let mut job = LayoutJob::default();
    job.wrap.max_width = wrap_width;
    let tag_col = Color32::from_rgb(110, 170, 255);
    let brk_col = Color32::from_rgb(255, 170, 80);
    let mut rest = text;
    let fmt = |c: Color32| TextFormat { font_id: font.clone(), color: c, ..Default::default() };
    while !rest.is_empty() {
        if rest.starts_with('{') {
            let end = rest.find('}').map(|e| e + 1).unwrap_or(rest.len());
            job.append(&rest[..end], 0.0, fmt(tag_col));
            rest = &rest[end..];
        } else if rest.starts_with("\\N") || rest.starts_with("\\n") || rest.starts_with("\\h") {
            job.append(&rest[..2], 0.0, fmt(brk_col));
            rest = &rest[2..];
        } else {
            let next = rest.find(['{', '\\']).filter(|&i| i > 0).unwrap_or_else(|| rest.char_indices().nth(1).map(|(i, _)| i).unwrap_or(rest.len()));
            job.append(&rest[..next], 0.0, fmt(base));
            rest = &rest[next..];
        }
    }
    job
}
