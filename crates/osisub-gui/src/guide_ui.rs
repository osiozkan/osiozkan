//! Uygulama içi kullanım kılavuzu (docs/KULLANIM.md ikiliye gömülü): basit Markdown görüntüleme
//! (başlık, kalın, italik, kod) ve paragraf süzme araması.
use eframe::egui::{self, text::LayoutJob, Color32, FontId, RichText, TextFormat};

pub const GUIDE: &str = include_str!("../../../docs/KULLANIM.md");

#[derive(Debug, PartialEq)]
pub enum Block<'a> {
    Heading(usize, &'a str),
    Code(String),
    Para(String),
}

/// Markdown'u bloklara ayırır: başlıklar, ``` kod blokları, boş satırla ayrılan paragraflar
/// (`**Başlık.**` ile başlayan satırlar ayrı paragraf sayılır).
pub fn blocks(md: &str) -> Vec<Block<'_>> {
    let mut out = Vec::new();
    let mut para: Vec<&str> = Vec::new();
    let mut code: Option<Vec<&str>> = None;
    let flush = |para: &mut Vec<&str>, out: &mut Vec<Block>| {
        if !para.is_empty() {
            out.push(Block::Para(para.join(" ")));
            para.clear();
        }
    };
    for line in md.lines() {
        if line.trim_start().starts_with("```") {
            match code.take() {
                Some(c) => out.push(Block::Code(c.join("\n"))),
                None => {
                    flush(&mut para, &mut out);
                    code = Some(Vec::new());
                }
            }
            continue;
        }
        if let Some(c) = code.as_mut() {
            c.push(line);
            continue;
        }
        let t = line.trim();
        if t.is_empty() {
            flush(&mut para, &mut out);
        } else if let Some(h) = t.strip_prefix("## ") {
            flush(&mut para, &mut out);
            out.push(Block::Heading(2, h));
        } else if let Some(h) = t.strip_prefix("# ") {
            flush(&mut para, &mut out);
            out.push(Block::Heading(1, h));
        } else if t.starts_with("**") || t.starts_with("- ") {
            flush(&mut para, &mut out);
            para.push(t);
        } else {
            para.push(t);
        }
    }
    flush(&mut para, &mut out);
    out
}

/// Satır içi Markdown → (metin, kalın, italik, kod) parçaları.
pub fn inline(s: &str) -> Vec<(String, bool, bool, bool)> {
    let mut out: Vec<(String, bool, bool, bool)> = Vec::new();
    let (mut b, mut i, mut c) = (false, false, false);
    let mut cur = String::new();
    let chars: Vec<char> = s.chars().collect();
    let mut k = 0;
    let push = |cur: &mut String, out: &mut Vec<(String, bool, bool, bool)>, b, i, c| {
        if !cur.is_empty() {
            out.push((std::mem::take(cur), b, i, c));
        }
    };
    while k < chars.len() {
        let ch = chars[k];
        if ch == '`' {
            push(&mut cur, &mut out, b, i, c);
            c = !c;
        } else if !c && ch == '*' && chars.get(k + 1) == Some(&'*') {
            push(&mut cur, &mut out, b, i, c);
            b = !b;
            k += 1;
        } else if !c && ch == '*' {
            push(&mut cur, &mut out, b, i, c);
            i = !i;
        } else if !c && ch == '[' {
            // [metin](bağlantı) → metin
            if let Some(close) = chars[k..].iter().position(|&x| x == ']') {
                if chars.get(k + close + 1) == Some(&'(') {
                    if let Some(end) = chars[k + close..].iter().position(|&x| x == ')') {
                        cur.extend(&chars[k + 1..k + close]);
                        k += close + end + 1;
                        continue;
                    }
                }
            }
            cur.push(ch);
        } else {
            cur.push(ch);
        }
        k += 1;
    }
    push(&mut cur, &mut out, b, i, c);
    out
}

fn para_job(ui: &egui::Ui, s: &str, width: f32) -> LayoutJob {
    let mut job = LayoutJob::default();
    job.wrap.max_width = width;
    let base = ui.visuals().text_color();
    let strong = ui.visuals().strong_text_color();
    for (t, b, i, c) in inline(s) {
        let mut f = TextFormat { color: if b { strong } else { base }, italics: i, font_id: FontId::proportional(14.0), ..Default::default() };
        if c {
            f.font_id = FontId::monospace(13.0);
            f.color = crate::theme::ACCENT;
        }
        job.append(&t, 0.0, f);
    }
    job
}

/// Kılavuz penceresi. `query`: arama kutusu (kapatılınca None).
pub fn show(ctx: &egui::Context, query: &mut Option<String>) {
    let Some(q) = query.as_mut() else { return };
    let mut open = true;
    egui::Window::new("Kullanım kılavuzu").open(&mut open).default_width(720.0).default_height(560.0).show(ctx, |ui| {
        ui.horizontal(|ui| {
            ui.label("Ara:");
            ui.add(egui::TextEdit::singleline(q).hint_text("ör. pipet, OCR, karaoke").desired_width(260.0));
            if !q.is_empty() && ui.small_button("Temizle").clicked() {
                q.clear();
            }
        });
        ui.separator();
        let needle = osisub_core::qc::tr_lower(q.trim());
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            let w = ui.available_width();
            let mut shown = 0;
            for b in blocks(GUIDE) {
                match &b {
                    Block::Heading(lvl, h) => {
                        if needle.is_empty() {
                            ui.add_space(6.0);
                            ui.label(RichText::new(*h).size(if *lvl == 1 { 20.0 } else { 17.0 }).strong());
                        }
                    }
                    Block::Code(c) => {
                        if needle.is_empty() || osisub_core::qc::tr_lower(c).contains(&needle) {
                            ui.label(RichText::new(c).monospace().color(Color32::from_gray(200)));
                            shown += 1;
                        }
                    }
                    Block::Para(p) => {
                        if needle.is_empty() || osisub_core::qc::tr_lower(p).contains(&needle) {
                            ui.label(para_job(ui, p, w));
                            ui.add_space(4.0);
                            shown += 1;
                        }
                    }
                }
            }
            if shown == 0 {
                ui.label(RichText::new("Eşleşen bölüm yok. Komut paleti (Ctrl+Shift+P) tüm komutları arar.").weak());
            }
        });
    });
    if !open {
        *query = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_guide_markdown() {
        let b = blocks("# Başlık\n\nbir\niki\n\n```\nkod\n```\n**Kalın.** devam\n- madde\n");
        assert_eq!(b, vec![Block::Heading(1, "Başlık"), Block::Para("bir iki".into()), Block::Code("kod".into()), Block::Para("**Kalın.** devam".into()), Block::Para("- madde".into())]);
        let i = inline("a **b** *c* `d` [e](http://x)");
        assert_eq!(i, vec![("a ".into(), false, false, false), ("b".into(), true, false, false), (" ".into(), false, false, false), ("c".into(), false, true, false), (" ".into(), false, false, false), ("d".into(), false, false, true), (" e".into(), false, false, false)]);
        // gömülü kılavuz ayrıştırılabilir ve bölümleri var
        assert!(blocks(GUIDE).iter().filter(|b| matches!(b, Block::Heading(..))).count() >= 2);
    }
}
