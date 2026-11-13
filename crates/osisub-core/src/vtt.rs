//! WebVTT okuma/yazma.
use crate::ass::{Dialogue, Script};
use crate::{srt, time};

pub fn parse(text: &str) -> Vec<Dialogue> {
    let text = text.strip_prefix('\u{feff}').unwrap_or(text).replace("\r\n", "\n");
    let mut body = text.as_str();
    if body.starts_with("WEBVTT") {
        body = body.split_once("\n\n").map(|x| x.1).unwrap_or("");
    }
    // Cue başlığı satırı (`id`) srt ayrıştırıcısı tarafından atlanır; NOTE/STYLE bloklarını ele.
    let cleaned: Vec<String> = body
        .split("\n\n")
        .filter(|b| !b.starts_with("NOTE") && !b.starts_with("STYLE") && !b.starts_with("REGION"))
        .map(cue_settings_to_an)
        .collect();
    srt::parse(&cleaned.join("\n\n"))
}

/// İpucu ayarlarını (`line:0 align:start`…) metnin başına `{\\anN}` olarak taşır.
fn cue_settings_to_an(block: &str) -> String {
    let mut lines: Vec<String> = block.lines().map(str::to_string).collect();
    let Some(ti) = lines.iter().position(|l| l.contains("-->")) else { return block.to_string() };
    let settings: Vec<&str> = lines[ti].split("-->").nth(1).unwrap_or("").split_whitespace().skip(1).collect();
    let mut row = 2; // 1 üst, 2 alt, 0 orta
    let mut col = 1; // 0 sol, 1 orta, 2 sağ
    for st in settings {
        if let Some(v) = st.strip_prefix("line:") {
            let v = v.split(',').next().unwrap_or("");
            if let Some(p) = v.strip_suffix('%').and_then(|p| p.parse::<f64>().ok()) {
                row = if p < 34.0 { 1 } else if p < 67.0 { 0 } else { 2 };
            } else if let Ok(n) = v.parse::<i32>() {
                row = if n >= 0 && n < 5 { 1 } else { 2 };
            }
        } else if let Some(v) = st.strip_prefix("align:") {
            col = match v {
                "start" | "left" => 0,
                "end" | "right" => 2,
                _ => 1,
            };
        }
    }
    let an = match row {
        1 => 7,
        0 => 4,
        _ => 1,
    } + col;
    if an != 2 && ti + 1 < lines.len() {
        lines[ti + 1] = format!("{{\\an{}}}{}", an, lines[ti + 1]);
    }
    lines.join("\n")
}

pub fn write(script: &Script) -> String {
    let mut out = String::from("WEBVTT\n\n");
    for d in script.dialogues().filter(|d| !d.comment) {
        let t = srt::ass_text_to_srt(&d.text);
        let (an, body) = srt::take_an_prefix(&t);
        // ASS \an → WebVTT ipucu ayarları (üst: line:0, sol/sağ hizalama)
        let mut cue = String::new();
        if let Some(a) = an {
            if a >= 7 {
                cue.push_str(" line:0");
            } else if (4..=6).contains(&a) {
                cue.push_str(" line:50%");
            }
            match a % 3 {
                1 => cue.push_str(" align:start"),
                0 => cue.push_str(" align:end"),
                _ => {}
            }
        }
        out.push_str(&format!("{} --> {}{}\n{}\n\n", time::format_vtt(d.start_time), time::format_vtt(d.end_time), cue, body));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn position_cue_settings() {
        let s = Script::from_dialogues(vec![
            Dialogue { start_time: 0, end_time: 1000, text: "{\\an8}Üst".into(), ..Default::default() },
            Dialogue { start_time: 1000, end_time: 2000, text: "{\\an1}Sol alt".into(), ..Default::default() },
            Dialogue { start_time: 2000, end_time: 3000, text: "Normal".into(), ..Default::default() },
        ]);
        let v = write(&s);
        assert!(v.contains("00:00:00.000 --> 00:00:01.000 line:0\nÜst"), "{v}");
        assert!(v.contains(" align:start\nSol alt"), "{v}");
        let back = parse(&v);
        assert_eq!(back[0].text, "{\\an8}Üst");
        assert_eq!(back[1].text, "{\\an1}Sol alt");
        assert_eq!(back[2].text, "Normal");
    }
    #[test]
    fn vtt() {
        let d = parse("WEBVTT\n\nNOTE x\n\ncue1\n00:00:01.000 --> 00:00:02.000 align:start\nHi\n");
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].end_time, 2000);
    }
}
