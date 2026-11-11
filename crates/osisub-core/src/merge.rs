//! Betik birleştirme (fansub "mux" adımı): diyalog + tabela + OP/ED karaoke dosyalarını tek
//! ASS'te toplar. Stil adı çakışmaları çözülür, farklı çözünürlükteki dosya önce yeniden
//! örneklenir, gömülü yazı tipleri taşınır, isteğe bağlı zaman kaydırması uygulanır.
use crate::ass::{style_line, Entry, Script, Style, SEC_EVENTS, SEC_STYLES};

#[derive(Clone, Debug, Default)]
pub struct MergeOptions {
    /// Eklenen dosyanın satırlarına uygulanacak kaydırma (ms; OP/ED yerleştirmek için)
    pub offset_ms: i64,
    /// Aynı adlı ama farklı tanımlı stiller için ek; boşsa " (2)", " (3)"… kullanılır
    pub style_suffix: String,
    /// Birleşik olayları başlangıç zamanına göre sırala
    pub sort: bool,
    /// Yalnızca diyalog satırlarını ekle (yorum satırlarını atla)
    pub skip_comments: bool,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct MergeReport {
    pub lines: usize,
    pub styles_added: usize,
    /// (eski ad, yeni ad)
    pub styles_renamed: Vec<(String, String)>,
    pub fonts_added: usize,
    pub resampled_from: Option<(i32, i32)>,
}

fn same_style(a: &Style, b: &Style) -> bool {
    let mut b2 = b.clone();
    b2.name = a.name.clone();
    style_line(a) == style_line(&b2)
}

/// `\r<Stil>` başvurularını yeniden adlandırır.
fn rename_r_refs(text: &str, map: &[(String, String)]) -> String {
    if map.is_empty() || !text.contains("\\r") {
        return text.to_string();
    }
    let mut segs = crate::overrides::parse(text);
    for s in segs.iter_mut() {
        if let crate::overrides::Seg::Block(tags) = s {
            for t in tags.iter_mut() {
                if t.name == "r" {
                    if let Some((_, new)) = map.iter().find(|(old, _)| old == t.arg.trim()) {
                        t.arg = new.clone();
                    }
                }
            }
        }
    }
    crate::overrides::to_string(&segs)
}

/// `other`'ı `base`'e ekler.
pub fn merge_into(base: &mut Script, other: &Script, opts: &MergeOptions) -> MergeReport {
    let mut rep = MergeReport::default();
    let mut other = other.clone();
    let (bw, bh) = base.res();
    let (ow, oh) = other.res();
    if (ow, oh) != (bw, bh) && ow > 0 && oh > 0 && bw > 0 && bh > 0 {
        crate::timing::resample(&mut other, bw, bh);
        rep.resampled_from = Some((ow, oh));
    }
    // Stiller
    let mut rename: Vec<(String, String)> = vec![];
    let mut new_styles: Vec<Entry> = vec![];
    for st in other.styles() {
        match base.style_by_name(&st.name) {
            Some(existing) if same_style(existing, st) => {}
            Some(_) => {
                let taken = |n: &str, base: &Script, extra: &[Entry]| base.style_by_name(n).is_some() || extra.iter().any(|e| matches!(e, Entry::Style(s) if s.name == n));
                let mut k = 2;
                let mut name = if opts.style_suffix.is_empty() { format!("{} ({})", st.name, k) } else { format!("{}{}", st.name, opts.style_suffix) };
                while taken(&name, base, &new_styles) {
                    k += 1;
                    name = format!("{} ({})", st.name, k);
                }
                let mut s2 = st.clone();
                s2.name = name.clone();
                rep.styles_renamed.push((st.name.clone(), name.clone()));
                rename.push((st.name.clone(), name));
                new_styles.push(Entry::Style(s2));
            }
            None => {
                new_styles.push(Entry::Style(st.clone()));
                rep.styles_added += 1;
            }
        }
    }
    if !new_styles.is_empty() {
        let at = base
            .entries
            .iter()
            .rposition(|e| e.section() == SEC_STYLES)
            .map(|i| i + 1)
            .or_else(|| base.entries.iter().position(|e| e.section() == SEC_EVENTS))
            .unwrap_or(base.entries.len());
        base.entries.splice(at..at, new_styles);
    }
    // Olaylar
    let mut lines: Vec<Entry> = vec![];
    for d in other.dialogues() {
        if opts.skip_comments && d.comment {
            continue;
        }
        let mut d = d.clone();
        if let Some((_, new)) = rename.iter().find(|(old, _)| *old == d.style) {
            d.style = new.clone();
        }
        d.text = rename_r_refs(&d.text, &rename);
        d.start_time = (d.start_time + opts.offset_ms).max(0);
        d.end_time = (d.end_time + opts.offset_ms).max(0);
        lines.push(Entry::Dialogue(d));
    }
    rep.lines = lines.len();
    let at = base.entries.iter().rposition(|e| e.section() == SEC_EVENTS).map(|i| i + 1).unwrap_or(base.entries.len());
    base.entries.splice(at..at, lines);
    if opts.sort {
        crate::qc::sort_by_start(base);
    }
    // Gömülü yazı tipleri/grafikler (aynı ad varsa atla)
    let have: Vec<String> = crate::attach::list(base).into_iter().map(|a| a.name).collect();
    for a in crate::attach::list(&other) {
        if have.contains(&a.name) {
            continue;
        }
        if let Some(data) = crate::attach::extract(&other, &a.name) {
            crate::attach::attach(base, &a.name, &data);
            rep.fonts_added += 1;
        }
    }
    rep
}

#[cfg(test)]
mod tests {
    use super::*;

    const HDR: &str = "[Script Info]\nPlayResX: {W}\nPlayResY: {H}\n\n[V4+ Styles]\nFormat: Name, Fontname, Fontsize, PrimaryColour, SecondaryColour, OutlineColour, BackColour, Bold, Italic, Underline, StrikeOut, ScaleX, ScaleY, Spacing, Angle, BorderStyle, Outline, Shadow, Alignment, MarginL, MarginR, MarginV, Encoding\n";
    fn style(name: &str, size: i32) -> String {
        format!("Style: {name},Arial,{size},&H00FFFFFF,&H000000FF,&H00000000,&H00000000,0,0,0,0,100,100,0,0,1,2,2,2,10,10,10,1\n")
    }
    fn script(w: i32, h: i32, styles: &str, events: &str) -> Script {
        Script::parse(&format!("{}{}\n[Events]\nFormat: Layer, Start, End, Style, Name, MarginL, MarginR, MarginV, Effect, Text\n{}", HDR.replace("{W}", &w.to_string()).replace("{H}", &h.to_string()), styles, events))
    }

    #[test]
    fn merges_styles_events_and_resamples() {
        let mut base = script(1920, 1080, &(style("Default", 60) + &style("Sign", 40)), "Dialogue: 0,0:00:01.00,0:00:02.00,Default,,0,0,0,,Merhaba\n");
        let other = script(
            1280,
            720,
            &(style("Default", 40) + &style("Sign", 30) + &style("OP", 30)),
            "Dialogue: 0,0:00:00.50,0:00:01.50,Sign,,0,0,0,,{\\pos(640,360)}Tabela\nDialogue: 0,0:00:03.00,0:00:04.00,OP,,0,0,0,,{\\rSign}la\nComment: 0,0:00:00.00,0:00:01.00,OP,,0,0,0,,not\n",
        );
        let rep = merge_into(&mut base, &other, &MergeOptions { offset_ms: 1000, sort: true, skip_comments: true, ..Default::default() });
        assert_eq!(rep.resampled_from, Some((1280, 720)));
        // 1280→1920 yeniden örnekleme kenar/gölge/boşlukları da ölçekler → aynı adlı stiller farklılaşır
        assert_eq!(rep.styles_renamed, vec![("Default".to_string(), "Default (2)".to_string()), ("Sign".to_string(), "Sign (2)".to_string())]);
        assert_eq!(rep.styles_added, 1);
        assert_eq!(rep.lines, 2);
        let names: Vec<&str> = base.styles().map(|s| s.name.as_str()).collect();
        assert_eq!(names, vec!["Default", "Sign", "Default (2)", "Sign (2)", "OP"]);
        let ev: Vec<(i64, &str, &str)> = base.dialogues().map(|d| (d.start_time, d.style.as_str(), d.text.as_str())).collect();
        assert_eq!(ev, vec![(1000, "Default", "Merhaba"), (1500, "Sign (2)", "{\\pos(960,540)}Tabela"), (4000, "OP", "{\\rSign (2)}la")]);
        // yazıp yeniden okuyunca aynı
        let again = Script::parse(&base.to_ass_string());
        assert_eq!(again.dialogues().count(), 3);
        assert_eq!(again.styles().count(), 5);
        // aynı çözünürlükte aynı tanımlı stil yeniden kullanılır
        let mut b2 = script(1920, 1080, &style("Default", 60), "");
        let r = merge_into(&mut b2, &script(1920, 1080, &style("Default", 60), "Dialogue: 0,0:00:01.00,0:00:02.00,Default,,0,0,0,,x\n"), &MergeOptions::default());
        assert!(r.styles_renamed.is_empty() && r.styles_added == 0 && r.lines == 1);
    }

    #[test]
    fn carries_fonts_once() {
        let mut base = script(1920, 1080, &style("Default", 60), "");
        let mut other = script(1920, 1080, &style("Default", 60), "Dialogue: 0,0:00:01.00,0:00:02.00,Default,,0,0,0,,x\n");
        crate::attach::attach(&mut other, "Font.ttf", b"fontdata-123");
        let r1 = merge_into(&mut base, &other, &MergeOptions::default());
        let r2 = merge_into(&mut base, &other, &MergeOptions::default());
        assert_eq!((r1.fonts_added, r2.fonts_added), (1, 0));
        assert_eq!(crate::attach::extract(&base, "Font_0.ttf").as_deref(), Some(&b"fontdata-123"[..]));
        assert_eq!(base.dialogues().count(), 2);
    }
}
