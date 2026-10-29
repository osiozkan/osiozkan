use eframe::egui::{self, Color32, FontData, FontDefinitions, FontFamily};
use std::sync::Arc;

pub const ACCENT: Color32 = Color32::from_rgb(0xe5, 0x6b, 0x9a);
pub const TAG_COLOR: Color32 = Color32::from_rgb(0x8a, 0x8f, 0xa8);
pub const BAD: Color32 = Color32::from_rgb(0xe0, 0x5a, 0x5a);
pub const WARN: Color32 = Color32::from_rgb(0xe0, 0xa8, 0x4a);

pub fn setup(ctx: &egui::Context) {
    apply(ctx, false);
    load_cjk_font(ctx);
}

pub fn apply(ctx: &egui::Context, light: bool) {
    let mut v = if light { egui::Visuals::light() } else { egui::Visuals::dark() };
    v.selection.bg_fill = if light { Color32::from_rgb(0xf3, 0xc4, 0xd7) } else { Color32::from_rgb(0x4a, 0x2a, 0x3c) };
    v.selection.stroke = egui::Stroke::new(1.0, ACCENT);
    v.hyperlink_color = ACCENT;
    v.window_corner_radius = egui::CornerRadius::same(8);
    ctx.set_visuals(v);
    ctx.global_style_mut(|s| {
        s.spacing.item_spacing = egui::vec2(8.0, 5.0);
        s.spacing.button_padding = egui::vec2(8.0, 4.0);
        // Hücre etiketleri seçilebilir olmasın: Ctrl+C satır kopyalamayı bozuyor, tıklamalar satıra gitsin.
        s.interaction.selectable_labels = false;
    });
}

/// Japonca/Çince karakterler için bulunan ilk sistem yazı tipini yedek olarak ekler.
fn load_cjk_font(ctx: &egui::Context) {
    let candidates = [
        "/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc",
        "/usr/share/fonts/truetype/noto/NotoSansCJK-Regular.ttc",
        "/usr/share/fonts/noto-cjk/NotoSansCJK-Regular.ttc",
        "/etc/alternatives/fonts-japanese-gothic.ttf",
        "/usr/share/fonts/truetype/takao-gothic/TakaoPGothic.ttf",
        "C:\\Windows\\Fonts\\YuGothR.ttc",
        "C:\\Windows\\Fonts\\msgothic.ttc",
        "C:\\Windows\\Fonts\\meiryo.ttc",
        "/System/Library/Fonts/ヒラギノ角ゴシック W3.ttc",
        "/System/Library/Fonts/Hiragino Sans GB.ttc",
    ];
    for p in candidates {
        if let Ok(bytes) = std::fs::read(p) {
            let mut fonts = FontDefinitions::default();
            fonts.font_data.insert("cjk".into(), Arc::new(FontData::from_owned(bytes)));
            for fam in [FontFamily::Proportional, FontFamily::Monospace] {
                fonts.families.entry(fam).or_default().push("cjk".into());
            }
            ctx.set_fonts(fonts);
            return;
        }
    }
}
