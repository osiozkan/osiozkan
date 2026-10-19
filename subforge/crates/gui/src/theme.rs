use eframe::egui::{self, Color32, FontData, FontDefinitions, FontFamily, Stroke};
use std::path::Path;

pub const ACCENT: Color32 = Color32::from_rgb(255, 138, 76); // kitsune orange
pub const OK: Color32 = Color32::from_rgb(98, 196, 120);
pub const WARN: Color32 = Color32::from_rgb(235, 190, 70);
pub const ERR: Color32 = Color32::from_rgb(235, 94, 94);

/// Add a CJK-capable system font as fallback so Japanese lines are readable.
pub fn install_fonts(ctx: &egui::Context) {
    let candidates = [
        "/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc",
        "/usr/share/fonts/noto-cjk/NotoSansCJK-Regular.ttc",
        "/usr/share/fonts/truetype/noto/NotoSansCJK-Regular.ttc",
        "/usr/share/fonts/truetype/wqy/wqy-microhei.ttc",
        "/usr/share/fonts/truetype/droid/DroidSansFallbackFull.ttf",
        "C:\\Windows\\Fonts\\YuGothM.ttc",
        "C:\\Windows\\Fonts\\meiryo.ttc",
        "C:\\Windows\\Fonts\\msgothic.ttc",
        "C:\\Windows\\Fonts\\msyh.ttc",
        "/System/Library/Fonts/PingFang.ttc",
        "/System/Library/Fonts/Hiragino Sans GB.ttc",
    ];
    let mut fonts = FontDefinitions::default();
    for p in candidates {
        if Path::new(p).exists() {
            if let Ok(bytes) = std::fs::read(p) {
                fonts.font_data.insert("cjk".into(), std::sync::Arc::new(FontData::from_owned(bytes)));
                for fam in [FontFamily::Proportional, FontFamily::Monospace] {
                    fonts.families.entry(fam).or_default().push("cjk".into());
                }
                break;
            }
        }
    }
    ctx.set_fonts(fonts);
}

pub fn apply(ctx: &egui::Context, dark: bool, scale: f32) {
    let mut v = if dark { egui::Visuals::dark() } else { egui::Visuals::light() };
    v.selection.bg_fill = Color32::from_rgba_unmultiplied(255, 138, 76, 70);
    v.selection.stroke = Stroke::new(1.0_f32, ACCENT);
    v.hyperlink_color = ACCENT;
    if dark {
        v.panel_fill = Color32::from_rgb(24, 25, 29);
        v.window_fill = Color32::from_rgb(30, 31, 36);
        v.extreme_bg_color = Color32::from_rgb(16, 17, 20);
        v.faint_bg_color = Color32::from_rgb(30, 31, 36);
        v.widgets.noninteractive.bg_stroke = Stroke::new(1.0_f32, Color32::from_rgb(48, 50, 58));
    }
    v.widgets.hovered.bg_stroke = Stroke::new(1.0_f32, ACCENT);
    v.widgets.active.bg_stroke = Stroke::new(1.0_f32, ACCENT);
    ctx.set_visuals(v);
    ctx.set_pixels_per_point(scale.clamp(0.75, 2.5));
    ctx.style_mut(|s| {
        s.spacing.item_spacing = egui::vec2(8.0, 5.0);
        s.spacing.button_padding = egui::vec2(8.0, 3.0);
        s.spacing.interact_size.y = 22.0;
    });
}
