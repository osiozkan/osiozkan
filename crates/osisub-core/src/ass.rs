//! ASS/SSA veri modeli, okuyucu ve yazıcı.
//!
//! Model Aegisub'ın Lua API'sindeki gibi düz bir satır listesidir; bölümler
//! satırların konumundan ve `section` alanından türetilir. Bilinmeyen bölümler
//! (Fonts, Graphics, Aegisub Project Garbage, Extradata ...) ham olarak korunur.

use crate::color::Color;
use crate::time;

pub const SEC_INFO: &str = "Script Info";
pub const SEC_STYLES: &str = "V4+ Styles";
pub const SEC_EVENTS: &str = "Events";
pub const SEC_GARBAGE: &str = "Aegisub Project Garbage";

#[derive(Clone, Debug, PartialEq)]
pub struct Style {
    pub name: String,
    pub fontname: String,
    pub fontsize: f64,
    pub color1: String,
    pub color2: String,
    pub color3: String,
    pub color4: String,
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    pub strikeout: bool,
    pub scale_x: f64,
    pub scale_y: f64,
    pub spacing: f64,
    pub angle: f64,
    pub borderstyle: i32,
    pub outline: f64,
    pub shadow: f64,
    pub align: i32,
    pub margin_l: i32,
    pub margin_r: i32,
    pub margin_t: i32,
    pub margin_b: i32,
    pub encoding: i32,
}

impl Default for Style {
    fn default() -> Self {
        Style {
            name: "Default".into(),
            fontname: "Arial".into(),
            fontsize: 48.0,
            color1: "&H00FFFFFF".into(),
            color2: "&H000000FF".into(),
            color3: "&H00000000".into(),
            color4: "&H00000000".into(),
            bold: false,
            italic: false,
            underline: false,
            strikeout: false,
            scale_x: 100.0,
            scale_y: 100.0,
            spacing: 0.0,
            angle: 0.0,
            borderstyle: 1,
            outline: 2.0,
            shadow: 2.0,
            align: 2,
            margin_l: 10,
            margin_r: 10,
            margin_t: 10,
            margin_b: 10,
            encoding: 1,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Dialogue {
    pub comment: bool,
    pub layer: i32,
    pub start_time: i64,
    pub end_time: i64,
    pub style: String,
    pub actor: String,
    pub margin_l: i32,
    pub margin_r: i32,
    pub margin_t: i32,
    pub margin_b: i32,
    pub effect: String,
    pub text: String,
}

impl Default for Dialogue {
    fn default() -> Self {
        Dialogue {
            comment: false,
            layer: 0,
            start_time: 0,
            end_time: 5000,
            style: "Default".into(),
            actor: String::new(),
            margin_l: 0,
            margin_r: 0,
            margin_t: 0,
            margin_b: 0,
            effect: String::new(),
            text: String::new(),
        }
    }
}

impl Dialogue {
    pub fn duration(&self) -> i64 {
        self.end_time - self.start_time
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum Entry {
    Info { key: String, value: String },
    Style(Style),
    Dialogue(Dialogue),
    /// Ham satır: bilinmeyen bölümler, yorumlar, boş satırlar.
    Unknown { section: String, raw: String },
}

impl Entry {
    pub fn section(&self) -> &str {
        match self {
            Entry::Info { .. } => SEC_INFO,
            Entry::Style(_) => SEC_STYLES,
            Entry::Dialogue(_) => SEC_EVENTS,
            Entry::Unknown { section, .. } => section,
        }
    }
}

#[derive(Clone, Debug)]
pub struct Script {
    pub entries: Vec<Entry>,
    pub crlf: bool,
}

impl Default for Script {
    fn default() -> Self {
        Script::new_default()
    }
}

fn fmt_num(v: f64) -> String {
    if (v - v.round()).abs() < 1e-9 {
        format!("{}", v.round() as i64)
    } else {
        let s = format!("{:.4}", v);
        s.trim_end_matches('0').trim_end_matches('.').to_string()
    }
}

fn b2s(b: bool) -> &'static str {
    if b { "-1" } else { "0" }
}

fn s2b(s: &str) -> bool {
    matches!(s.trim().parse::<i32>(), Ok(v) if v != 0)
}

impl Script {
    pub fn new_default() -> Script {
        let mut s = Script { entries: vec![], crlf: true };
        for (k, v) in [
            ("ScriptType", "v4.00+"),
            ("WrapStyle", "0"),
            ("ScaledBorderAndShadow", "yes"),
            ("PlayResX", "1920"),
            ("PlayResY", "1080"),
            ("YCbCr Matrix", "TV.709"),
        ] {
            s.entries.push(Entry::Info { key: k.into(), value: v.into() });
        }
        s.entries.push(Entry::Style(Style::default()));
        s
    }

    pub fn info(&self, key: &str) -> Option<&str> {
        self.entries.iter().find_map(|e| match e {
            Entry::Info { key: k, value } if k.eq_ignore_ascii_case(key) => Some(value.as_str()),
            _ => None,
        })
    }

    pub fn set_info(&mut self, key: &str, value: &str) {
        for e in self.entries.iter_mut() {
            if let Entry::Info { key: k, value: v } = e {
                if k.eq_ignore_ascii_case(key) {
                    *v = value.to_string();
                    return;
                }
            }
        }
        let pos = self
            .entries
            .iter()
            .rposition(|e| matches!(e, Entry::Info { .. }))
            .map(|i| i + 1)
            .unwrap_or(0);
        self.entries.insert(pos, Entry::Info { key: key.into(), value: value.into() });
    }

    /// `[Aegisub Project Garbage]` bölümündeki `Anahtar: değer` satırı.
    pub fn garbage(&self, key: &str) -> Option<&str> {
        self.entries.iter().find_map(|e| match e {
            Entry::Unknown { section, raw } if section == SEC_GARBAGE => {
                let (k, v) = raw.split_once(':')?;
                k.trim().eq_ignore_ascii_case(key).then(|| v.trim())
            }
            _ => None,
        })
    }

    /// Garbage bölümünde anahtarı ayarlar; bölüm yoksa Script Info'dan sonra oluşturur.
    pub fn set_garbage(&mut self, key: &str, value: &str) {
        let line = format!("{}: {}", key, value);
        for e in self.entries.iter_mut() {
            if let Entry::Unknown { section, raw } = e {
                if section == SEC_GARBAGE {
                    if let Some((k, _)) = raw.split_once(':') {
                        if k.trim().eq_ignore_ascii_case(key) {
                            *raw = line;
                            return;
                        }
                    }
                }
            }
        }
        let pos = self
            .entries
            .iter()
            .rposition(|e| matches!(e, Entry::Unknown { section, .. } if section == SEC_GARBAGE))
            .map(|i| i + 1)
            .or_else(|| self.entries.iter().rposition(|e| e.section() == SEC_INFO).map(|i| i + 1))
            .unwrap_or(0);
        self.entries.insert(pos, Entry::Unknown { section: SEC_GARBAGE.to_string(), raw: line });
    }

    pub fn res(&self) -> (i32, i32) {
        let x = self.info("PlayResX").and_then(|v| v.trim().parse().ok());
        let y = self.info("PlayResY").and_then(|v| v.trim().parse().ok());
        match (x, y) {
            (Some(x), Some(y)) => (x, y),
            (Some(x), None) => (x, if x == 1280 { 720 } else { x * 3 / 4 }),
            (None, Some(y)) => (if y == 1024 { 1280 } else { y * 4 / 3 }, y),
            _ => (384, 288),
        }
    }

    pub fn styles(&self) -> impl Iterator<Item = &Style> {
        self.entries.iter().filter_map(|e| if let Entry::Style(s) = e { Some(s) } else { None })
    }

    pub fn dialogues(&self) -> impl Iterator<Item = &Dialogue> {
        self.entries.iter().filter_map(|e| if let Entry::Dialogue(d) = e { Some(d) } else { None })
    }

    pub fn dialogues_mut(&mut self) -> impl Iterator<Item = &mut Dialogue> {
        self.entries.iter_mut().filter_map(|e| if let Entry::Dialogue(d) = e { Some(d) } else { None })
    }

    pub fn style_by_name(&self, name: &str) -> Option<&Style> {
        self.styles().find(|s| s.name == name)
    }

    /// Yalnızca diyalog satırlarını (yorumlar dahil) kopya olarak verir.
    pub fn events(&self) -> Vec<Dialogue> {
        self.dialogues().cloned().collect()
    }

    pub fn from_dialogues(dialogues: Vec<Dialogue>) -> Script {
        let mut s = Script::new_default();
        s.entries.extend(dialogues.into_iter().map(Entry::Dialogue));
        s
    }

    // ---------------------------------------------------------------- okuma

    pub fn parse(text: &str) -> Script {
        let crlf = text.contains("\r\n");
        let text = text.strip_prefix('\u{feff}').unwrap_or(text);
        let mut entries = Vec::new();
        let mut section = String::new();
        let mut style_fmt: Vec<String> = default_style_format();
        let mut event_fmt: Vec<String> = default_event_format();
        let mut ssa = false;
        let mut pending_blank = 0usize;

        for raw in text.lines() {
            let line = raw.trim_end_matches('\r');
            let trimmed = line.trim();
            if trimmed.is_empty() {
                // Bölüm sonu boşluklarını yazarken yeniden üretiyoruz; yalnızca
                // bilinmeyen bölümlerin içindeki boşlukları koru.
                pending_blank += 1;
                continue;
            }
            if trimmed.starts_with('[') && trimmed.ends_with(']') {
                section = trimmed[1..trimmed.len() - 1].trim().to_string();
                pending_blank = 0;
                // Bölüm adları büyük/küçük harf duyarsızdır; kanonik biçime çevir.
                if section.eq_ignore_ascii_case("V4 Styles") {
                    ssa = true;
                    section = SEC_STYLES.to_string();
                } else if section.eq_ignore_ascii_case(SEC_INFO) {
                    section = SEC_INFO.to_string();
                } else if section.eq_ignore_ascii_case(SEC_STYLES) {
                    section = SEC_STYLES.to_string();
                } else if section.eq_ignore_ascii_case(SEC_EVENTS) {
                    section = SEC_EVENTS.to_string();
                }
                continue;
            }
            let lead = line.trim_start(); // metin sonundaki boşluklar korunur (yalnızca baştaki atılır)
            let known = matches!(section.as_str(), SEC_INFO | SEC_STYLES | SEC_EVENTS);
            if !known && pending_blank > 0 && !section.is_empty() {
                for _ in 0..pending_blank {
                    entries.push(Entry::Unknown { section: section.clone(), raw: String::new() });
                }
            }
            pending_blank = 0;
            match section.as_str() {
                SEC_INFO => {
                    if trimmed.starts_with(';') {
                        entries.push(Entry::Unknown { section: section.clone(), raw: line.to_string() });
                    } else if let Some((k, v)) = line.split_once(':') {
                        entries.push(Entry::Info { key: k.trim().to_string(), value: v.trim().to_string() });
                    } else {
                        entries.push(Entry::Unknown { section: section.clone(), raw: line.to_string() });
                    }
                }
                SEC_STYLES => {
                    if let Some(f) = strip_key(lead, "Format") {
                        style_fmt = f.split(',').map(|x| x.trim().to_ascii_lowercase()).collect();
                    } else if let Some(v) = strip_key(lead, "Style") {
                        if let Some(st) = parse_style(v, &style_fmt, ssa) {
                            entries.push(Entry::Style(st));
                        }
                    } else {
                        entries.push(Entry::Unknown { section: section.clone(), raw: line.to_string() });
                    }
                }
                SEC_EVENTS => {
                    if let Some(f) = strip_key(lead, "Format") {
                        event_fmt = f.split(',').map(|x| x.trim().to_ascii_lowercase()).collect();
                    } else if let Some(v) = strip_key(lead, "Dialogue") {
                        if let Some(d) = parse_event(v, &event_fmt, false) {
                            entries.push(Entry::Dialogue(d));
                        }
                    } else if let Some(v) = strip_key(lead, "Comment") {
                        if let Some(d) = parse_event(v, &event_fmt, true) {
                            entries.push(Entry::Dialogue(d));
                        }
                    } else {
                        // Picture/Sound/Movie/Command satırları: ham sakla.
                        entries.push(Entry::Unknown { section: section.clone(), raw: line.to_string() });
                    }
                }
                _ => entries.push(Entry::Unknown { section: section.clone(), raw: line.to_string() }),
            }
        }
        Script { entries, crlf }
    }

    pub fn parse_bytes(bytes: &[u8]) -> Script {
        Script::parse(&decode_bytes(bytes))
    }

    // ---------------------------------------------------------------- yazma

    pub fn to_ass_string(&self) -> String {
        self.to_ass_string_where(|_| true)
    }

    /// Yalnızca `keep` diyalogları ve tüm diğer girdilerle yazar (önizleme için küçültülmüş betik).
    pub fn to_ass_string_where(&self, keep: impl Fn(&Dialogue) -> bool) -> String {
        let nl = if self.crlf { "\r\n" } else { "\n" };
        let mut out = String::with_capacity(self.entries.len() * 80);
        let mut cur: Option<&str> = None;
        for e in &self.entries {
            if let Entry::Dialogue(d) = e {
                if !keep(d) {
                    continue;
                }
            }
            let sec = e.section();
            if cur != Some(sec) {
                if cur.is_some() {
                    out.push_str(nl);
                }
                out.push('[');
                out.push_str(sec);
                out.push(']');
                out.push_str(nl);
                match e {
                    Entry::Style(_) => {
                        out.push_str("Format: Name, Fontname, Fontsize, PrimaryColour, SecondaryColour, OutlineColour, BackColour, Bold, Italic, Underline, StrikeOut, ScaleX, ScaleY, Spacing, Angle, BorderStyle, Outline, Shadow, Alignment, MarginL, MarginR, MarginV, Encoding");
                        out.push_str(nl);
                    }
                    Entry::Dialogue(_) => {
                        out.push_str("Format: Layer, Start, End, Style, Name, MarginL, MarginR, MarginV, Effect, Text");
                        out.push_str(nl);
                    }
                    _ => {}
                }
                cur = Some(sec);
            }
            match e {
                Entry::Info { key, value } => {
                    out.push_str(key);
                    out.push_str(": ");
                    out.push_str(value);
                }
                Entry::Style(s) => out.push_str(&style_line(s)),
                Entry::Dialogue(d) => out.push_str(&dialogue_line(d)),
                Entry::Unknown { raw, .. } => out.push_str(raw),
            }
            out.push_str(nl);
        }
        out
    }
}

pub fn style_line(s: &Style) -> String {
    format!(
        "Style: {},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{}",
        s.name, s.fontname, fmt_num(s.fontsize), s.color1, s.color2, s.color3, s.color4,
        b2s(s.bold), b2s(s.italic), b2s(s.underline), b2s(s.strikeout),
        fmt_num(s.scale_x), fmt_num(s.scale_y), fmt_num(s.spacing), fmt_num(s.angle),
        s.borderstyle, fmt_num(s.outline), fmt_num(s.shadow), s.align,
        s.margin_l, s.margin_r, s.margin_t, s.encoding
    )
}

pub fn dialogue_line(d: &Dialogue) -> String {
    format!(
        "{}: {},{},{},{},{},{},{},{},{},{}",
        if d.comment { "Comment" } else { "Dialogue" },
        d.layer,
        time::format_ass(d.start_time),
        time::format_ass(d.end_time),
        d.style,
        d.actor,
        d.margin_l,
        d.margin_r,
        d.margin_t,
        d.effect,
        d.text
    )
}

fn strip_key<'a>(line: &'a str, key: &str) -> Option<&'a str> {
    let (k, v) = line.split_once(':')?;
    if k.trim().eq_ignore_ascii_case(key) {
        Some(v.trim_start())
    } else {
        None
    }
}

fn default_style_format() -> Vec<String> {
    "name,fontname,fontsize,primarycolour,secondarycolour,outlinecolour,backcolour,bold,italic,underline,strikeout,scalex,scaley,spacing,angle,borderstyle,outline,shadow,alignment,marginl,marginr,marginv,encoding"
        .split(',').map(String::from).collect()
}

fn default_event_format() -> Vec<String> {
    "layer,start,end,style,name,marginl,marginr,marginv,effect,text".split(',').map(String::from).collect()
}

fn parse_style(v: &str, fmt: &[String], ssa: bool) -> Option<Style> {
    let parts: Vec<&str> = v.splitn(fmt.len(), ',').collect();
    let mut st = Style::default();
    let mut marginv: Option<i32> = None;
    for (i, f) in fmt.iter().enumerate() {
        let val = parts.get(i).map(|s| s.trim()).unwrap_or("");
        let num = || val.parse::<f64>().unwrap_or(0.0);
        let int = || val.parse::<f64>().map(|x| x as i32).unwrap_or(0);
        match f.as_str() {
            "name" => st.name = val.to_string(),
            "fontname" => st.fontname = val.to_string(),
            "fontsize" => st.fontsize = num(),
            "primarycolour" => st.color1 = norm_color(val),
            "secondarycolour" => st.color2 = norm_color(val),
            "tertiarycolour" | "outlinecolour" => st.color3 = norm_color(val),
            "backcolour" => st.color4 = norm_color(val),
            "bold" => st.bold = s2b(val),
            "italic" => st.italic = s2b(val),
            "underline" => st.underline = s2b(val),
            "strikeout" => st.strikeout = s2b(val),
            "scalex" => st.scale_x = num(),
            "scaley" => st.scale_y = num(),
            "spacing" => st.spacing = num(),
            "angle" => st.angle = num(),
            "borderstyle" => st.borderstyle = int(),
            "outline" => st.outline = num(),
            "shadow" => st.shadow = num(),
            "alignment" => {
                let a = int();
                st.align = if ssa { ssa_align(a) } else { a };
            }
            "marginl" => st.margin_l = int(),
            "marginr" => st.margin_r = int(),
            "marginv" => marginv = Some(int()),
            "margint" => st.margin_t = int(),
            "marginb" => st.margin_b = int(),
            "encoding" => st.encoding = int(),
            _ => {}
        }
    }
    if let Some(m) = marginv {
        st.margin_t = m;
        st.margin_b = m;
    }
    Some(st)
}

fn ssa_align(a: i32) -> i32 {
    match a {
        1..=3 => a,
        5..=7 => a + 2,
        9..=11 => a - 5,
        _ => 2,
    }
}

fn norm_color(v: &str) -> String {
    match Color::parse(v) {
        Some(c) => c.to_style_string(),
        None => "&H00FFFFFF".into(),
    }
}

fn parse_event(v: &str, fmt: &[String], comment: bool) -> Option<Dialogue> {
    let parts: Vec<&str> = v.splitn(fmt.len(), ',').collect();
    if parts.len() < fmt.len() {
        return None;
    }
    let mut d = Dialogue { comment, ..Default::default() };
    let mut marginv: Option<i32> = None;
    for (i, f) in fmt.iter().enumerate() {
        let raw = parts[i];
        let val = raw.trim();
        let int = || val.parse::<f64>().map(|x| x as i32).unwrap_or(0);
        match f.as_str() {
            "layer" => d.layer = int(),
            "marked" => d.layer = 0,
            "start" => d.start_time = time::parse_ass(val)?,
            "end" => d.end_time = time::parse_ass(val)?,
            "style" => d.style = val.to_string(),
            "name" | "actor" => d.actor = val.to_string(),
            "marginl" => d.margin_l = int(),
            "marginr" => d.margin_r = int(),
            "marginv" => marginv = Some(int()),
            "margint" => d.margin_t = int(),
            "marginb" => d.margin_b = int(),
            "effect" => d.effect = val.to_string(),
            "text" => d.text = raw.to_string(),
            _ => {}
        }
    }
    if let Some(m) = marginv {
        d.margin_t = m;
        d.margin_b = m;
    }
    Some(d)
}

// ------------------------------------------------------------------ kodlama

/// UTF-8/UTF-16 (BOM'lu) ve BOM'suz eski Türkçe (Windows-1254) metinleri çözer.
pub fn decode_bytes(b: &[u8]) -> String {
    if b.starts_with(&[0xEF, 0xBB, 0xBF]) {
        return String::from_utf8_lossy(&b[3..]).into_owned();
    }
    if b.starts_with(&[0xFF, 0xFE]) || b.starts_with(&[0xFE, 0xFF]) {
        let le = b[0] == 0xFF;
        let u: Vec<u16> = b[2..]
            .chunks_exact(2)
            .map(|c| if le { u16::from_le_bytes([c[0], c[1]]) } else { u16::from_be_bytes([c[0], c[1]]) })
            .collect();
        return String::from_utf16_lossy(&u);
    }
    match std::str::from_utf8(b) {
        Ok(s) => s.to_string(),
        Err(_) => {
            // UTF-8 değil: kodlamayı tahmin et (Firefox'un chardetng'i; belirsizse Türkçe'ye eğilimli).
            let mut det = chardetng::EncodingDetector::new();
            det.feed(b, true);
            let enc = det.guess(Some(b"tr"), false);
            if enc == encoding_rs::WINDOWS_1254 || enc == encoding_rs::WINDOWS_1252 || enc == encoding_rs::ISO_8859_2 {
                // Tek baytlı Latin ailesi: Türkçe harfleri doğru eşleyen tablomuz var.
                b.iter().map(|&c| cp1254(c)).collect()
            } else {
                enc.decode(b).0.into_owned()
            }
        }
    }
}

fn cp1254(c: u8) -> char {
    match c {
        0xD0 => 'Ğ',
        0xDD => 'İ',
        0xDE => 'Ş',
        0xF0 => 'ğ',
        0xFD => 'ı',
        0xFE => 'ş',
        0x80 => '€',
        0x82 => '‚',
        0x83 => 'ƒ',
        0x84 => '„',
        0x85 => '…',
        0x86 => '†',
        0x87 => '‡',
        0x88 => 'ˆ',
        0x89 => '‰',
        0x8A => 'Š',
        0x8B => '‹',
        0x8C => 'Œ',
        0x91 => '‘',
        0x92 => '’',
        0x93 => '“',
        0x94 => '”',
        0x95 => '•',
        0x96 => '–',
        0x97 => '—',
        0x98 => '˜',
        0x99 => '™',
        0x9A => 'š',
        0x9B => '›',
        0x9C => 'œ',
        0x9F => 'Ÿ',
        _ => c as char,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "[Script Info]\nTitle: t\nPlayResX: 1280\nPlayResY: 720\n\n[V4+ Styles]\nFormat: Name, Fontname, Fontsize, PrimaryColour, SecondaryColour, OutlineColour, BackColour, Bold, Italic, Underline, StrikeOut, ScaleX, ScaleY, Spacing, Angle, BorderStyle, Outline, Shadow, Alignment, MarginL, MarginR, MarginV, Encoding\nStyle: Default,Arial,20,&H00FFFFFF,&H000000FF,&H00000000,&H00000000,0,0,0,0,100,100,0,0,1,2,2,2,10,10,10,1\n\n[Events]\nFormat: Layer, Start, End, Style, Name, MarginL, MarginR, MarginV, Effect, Text\nDialogue: 0,0:00:01.00,0:00:03.00,Default,,0,0,0,,Merhaba, dünya\nComment: 0,0:00:03.00,0:00:04.00,Default,x,0,0,0,,{\\k10}not\n\n[Aegisub Project Garbage]\nAudio File: a.mka\n";

    #[test]
    fn parse_and_write_roundtrip() {
        let s = Script::parse(SAMPLE);
        assert_eq!(s.res(), (1280, 720));
        assert_eq!(s.dialogues().count(), 2);
        assert_eq!(s.dialogues().next().unwrap().text, "Merhaba, dünya");
        let out = s.to_ass_string();
        let s2 = Script::parse(&out);
        assert_eq!(s.entries, s2.entries);
        assert!(out.contains("[Aegisub Project Garbage]"));
    }

    #[test]
    fn autodetect_non_utf8() {
        let tr = [0xDD, b's', b't', b'a', b'n', b'b', b'u', b'l', b' ', b'g', 0xFC, b'z', b'e', b'l', b'.'];
        assert_eq!(decode_bytes(&tr), "İstanbul gÜzel.".replace("Ü", "ü"));
        // Shift-JIS Japonca (uzun metin; tespit güvenilir olsun)
        let sj: Vec<u8> = [0x93u8, 0xFA, 0x96, 0x7B, 0x8C, 0xEA, 0x82, 0xCC, 0x8E, 0x9A, 0x96, 0x8B, 0x82, 0xAA, 0x82, 0xB1, 0x82, 0xB1, 0x82, 0xC9, 0x82, 0xA0, 0x82, 0xE9].repeat(8);
        let out = decode_bytes(&sj);
        assert!(out.contains("日本語"), "{}", out);
    }

    #[test]
    fn cp1254_decode() {
        assert_eq!(decode_bytes(&[0x59, 0xFD, 0x6C, 0xFE, 0xDD]), "YılşI".replace('I', "İ"));
    }
}

#[cfg(test)]
mod roundtrip_tests {
    use super::*;

    const AEGI: &str = "[Script Info]\n; Script generated by Aegisub 3.2.2\n; http://www.aegisub.org/\nTitle: Default Aegisub file\nScriptType: v4.00+\nWrapStyle: 0\nScaledBorderAndShadow: yes\nYCbCr Matrix: None\nPlayResX: 640\nPlayResY: 480\n\n[Aegisub Project Garbage]\nLast Style Storage: Default\nVideo File: ?dummy:23.976000:40000:640:480:47:163:254:\nVideo AR Mode: 4\nActive Line: 1\nVideo Position: 0\n\n[V4+ Styles]\nFormat: Name, Fontname, Fontsize, PrimaryColour, SecondaryColour, OutlineColour, BackColour, Bold, Italic, Underline, StrikeOut, ScaleX, ScaleY, Spacing, Angle, BorderStyle, Outline, Shadow, Alignment, MarginL, MarginR, MarginV, Encoding\nStyle: Default,Arial,20,&H00FFFFFF,&H000000FF,&H00000000,&H00000000,0,0,0,0,100,100,0,0,1,2,2,2,10,10,10,1\nStyle: Alt,Noto Sans,22.5,&H00FFFFFF,&H000000FF,&H00000000,&H80000000,-1,-1,0,0,100,100,0.5,0,1,2.25,0,8,10,10,10,1\n\n[Events]\nFormat: Layer, Start, End, Style, Name, MarginL, MarginR, MarginV, Effect, Text\nDialogue: 0,0:00:00.00,0:00:05.00,Default,,0,0,0,,Hello, {\\i1}world{\\i0}\nComment: 1,0:00:05.00,0:00:06.50,Alt,Ali,10,20,30,karaoke,{\\k20}test\n\n[Fonts]\nfontname: Arial_0.ttf\n!!!!!\n##@@@\n\n[Aegisub Extradata]\nData: 1,foo,bar\n";

    #[test]
    fn filtered_writer_keeps_headers() {
        let s = Script::parse(AEGI);
        let only_alt = s.to_ass_string_where(|d| d.style == "Alt");
        assert!(only_alt.contains("Style: Alt,") && only_alt.contains("Comment: 1,") && !only_alt.contains("Dialogue: 0,0:00:00.00"));
        let none = s.to_ass_string_where(|_| false);
        assert!(none.contains("[Events]") == false || !none.contains("Dialogue:"));
        assert!(Script::parse(&none).styles().count() == 2);
    }

    #[test]
    fn aegisub_file_is_byte_stable() {
        let s = Script::parse(AEGI);
        assert_eq!(s.to_ass_string().replace("\r\n", "\n"), AEGI);
        // CRLF korunur
        let crlf = AEGI.replace('\n', "\r\n");
        let s2 = Script::parse(&crlf);
        assert!(s2.crlf);
        assert_eq!(s2.to_ass_string(), crlf);
    }

    /// Rastgele (virgül, süslü parantez, Unicode, boşluk içeren) alanlarla yaz→oku gidiş-dönüşü.
    #[test]
    fn fuzz_roundtrip() {
        let mut seed: u64 = 0xDEADBEEFCAFEF00D;
        let mut rnd = move |n: usize| {
            seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            ((seed >> 33) as usize) % n.max(1)
        };
        let alphabet: Vec<char> = "abcçdefgğhıijklmnoöprsştuüvyzABCÇDEFGĞHIİJKLMNOÖPRSŞTUÜVYZ0123456789 ,.;:!?-_(){}[]\\/|\"'日本語ｶﾅ♪…".chars().collect();
        for round in 0..200 {
            let mut sc = Script::new_default();
            sc.entries.retain(|e| !matches!(e, Entry::Style(_)));
            let n_styles = 1 + rnd(3);
            for i in 0..n_styles {
                let name: String = (0..1 + rnd(8)).map(|_| alphabet[rnd(alphabet.len())]).filter(|c| *c != ',' && *c != '\n').collect::<String>().trim().to_string();
                let name = if name.is_empty() { format!("S{}", i) } else { name };
                let mut st = Style::default();
                st.name = name;
                st.fontname = ["Arial", "Noto Sans CJK JP", "MS Gothic"][rnd(3)].into();
                st.fontsize = (10 + rnd(90)) as f64 / if rnd(2) == 0 { 1.0 } else { 4.0 };
                st.bold = rnd(2) == 0;
                st.align = 1 + rnd(9) as i32;
                sc.entries.push(Entry::Style(st));
            }
            for _ in 0..rnd(20) {
                let text: String = (0..rnd(60)).map(|_| alphabet[rnd(alphabet.len())]).collect();
                let text = text.replace('\n', " ");
                let start = (rnd(100_000) / 10 * 10) as i64;
                let first_style = sc.styles().next().unwrap().name.clone();
                sc.entries.push(Entry::Dialogue(Dialogue {
                    comment: rnd(5) == 0,
                    layer: rnd(4) as i32,
                    start_time: start,
                    end_time: start + (rnd(5000) / 10 * 10) as i64,
                    style: first_style,
                    actor: (0..rnd(6)).map(|_| alphabet[rnd(alphabet.len())]).filter(|c| *c != ',').collect::<String>().trim().to_string(),
                    margin_l: rnd(50) as i32,
                    margin_r: rnd(50) as i32,
                    margin_t: rnd(50) as i32,
                    margin_b: 0,
                    effect: String::new(),
                    text,
                }));
            }
            // margin_b yazımda margin_t ile aynıdır (tek MarginV) — beklenen durumu eşitle
            for e in sc.entries.iter_mut() {
                if let Entry::Dialogue(d) = e {
                    d.margin_b = d.margin_t;
                }
            }
            let out = sc.to_ass_string();
            let back = Script::parse(&out);
            assert_eq!(back.entries, sc.entries, "tur {}", round);
            assert_eq!(back.to_ass_string(), out, "ikinci yazım kararlı olmalı (tur {})", round);
        }
    }

    #[test]
    fn garbage_get_set() {
        let mut s = Script::parse(AEGI);
        assert_eq!(s.garbage("video file"), Some("?dummy:23.976000:40000:640:480:47:163:254:"));
        s.set_garbage("Video File", "../film.mkv");
        s.set_garbage("Audio File", "../film.mkv");
        assert_eq!(s.garbage("Video File"), Some("../film.mkv"));
        assert_eq!(s.garbage("Audio File"), Some("../film.mkv"));
        let out = s.to_ass_string();
        assert!(out.contains("Video File: ../film.mkv") && out.contains("Audio File: ../film.mkv"));
        // bölüm yoksa oluşturulur ve Script Info'dan sonra gelir
        let mut n = Script::new_default();
        n.set_garbage("Active Line", "3");
        let t = n.to_ass_string();
        assert!(t.find("[Aegisub Project Garbage]").unwrap() > t.find("[Script Info]").unwrap());
        assert!(t.find("[Aegisub Project Garbage]").unwrap() < t.find("[V4+ Styles]").unwrap());
        assert_eq!(Script::parse(&t).garbage("Active Line"), Some("3"));
    }

    #[test]
    fn tolerant_inputs() {
        // BOM, boşluklu anahtarlar, farklı büyük/küçük harf, SSA Marked, eksik Format
        let t = "\u{feff}[script info]\nTitle:  X \n\n[V4 Styles]\nFormat: Name, Fontname, Fontsize, PrimaryColour, SecondaryColour, TertiaryColour, BackColour, Bold, Italic, BorderStyle, Outline, Shadow, Alignment, MarginL, MarginR, MarginV, AlphaLevel, Encoding\nStyle: Default,Arial,20,16777215,255,0,0,0,0,1,2,2,6,10,10,10,0,0\n\n[Events]\nFormat: Marked, Start, End, Style, Name, MarginL, MarginR, MarginV, Effect, Text\nDialogue: Marked=0,0:00:01.00,0:00:02.00,Default,,0,0,0,,x,y\n";
        let s = Script::parse(t);
        assert_eq!(s.info("Title"), Some("X"));
        let st = s.styles().next().unwrap();
        assert_eq!(st.align, 8, "SSA hizalama 6 → ASS 8");
        assert_eq!(st.color1, "&H00FFFFFF");
        assert_eq!(s.dialogues().next().unwrap().text, "x,y");
    }
}
