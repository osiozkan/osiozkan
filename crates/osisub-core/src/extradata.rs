//! Aegisub Extradata: `[Aegisub Extradata]` bölümü ve satırlara bağlı `{=1=2}` kimlikleri.
//!
//! Aegisub satır başına anahtar/değer verisi (Lua'da `line.extra`) saklar: değerler
//! `Data: kimlik,anahtar,kodlanmış-değer` satırlarında, diyalog metninin başında da
//! `{=kimlik=kimlik}` ön eki bulunur. Değer kodlaması: `e` + satır içi kaçış (`#XX`)
//! ya da `u` + Aegisub'ın uuencode biçimi. Ön ek ayrıştırmada `Dialogue::extra`'ya alınır,
//! yazarken geri eklenir; böylece arayüzde/QC'de görünmez.
use crate::ass::{Entry, Script};
use std::collections::BTreeMap;

pub const SEC_EXTRADATA: &str = "Aegisub Extradata";

/// Metin başındaki `{=1=2}` ön ekini ayırır: (kimlikler, kalan metin).
pub fn split_prefix(text: &str) -> (Vec<u32>, &str) {
    let Some(rest) = text.strip_prefix("{=") else { return (Vec::new(), text) };
    let Some(end) = rest.find('}') else { return (Vec::new(), text) };
    let body = &rest[..end];
    let mut ids = Vec::new();
    for part in body.split('=') {
        if part.is_empty() || !part.bytes().all(|b| b.is_ascii_digit()) {
            return (Vec::new(), text);
        }
        match part.parse() {
            Ok(n) => ids.push(n),
            Err(_) => return (Vec::new(), text),
        }
    }
    (ids, &rest[end + 1..])
}

pub fn prefix(ids: &[u32]) -> String {
    let mut s = String::from("{");
    for id in ids {
        s.push('=');
        s.push_str(&id.to_string());
    }
    s.push('}');
    s
}

fn is_special(b: u8) -> bool {
    b <= 0x1F || b == b'#' || b == b',' || b == b':' || b == b'|'
}

/// Aegisub `inline_string_encode`: denetim karakterleri ve `# , : |` → `#XX`.
pub fn inline_encode(s: &str) -> String {
    let mut out = Vec::with_capacity(s.len());
    for &b in s.as_bytes() {
        if is_special(b) {
            out.extend_from_slice(format!("#{:02X}", b).as_bytes());
        } else {
            out.push(b);
        }
    }
    // Yalnızca ASCII baytlar değiştirildi: UTF-8 geçerliliği korunur.
    String::from_utf8(out).unwrap_or_default()
}

pub fn inline_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'#' && i + 2 < b.len() {
            let hex = |c: u8| (c as char).to_digit(16);
            if let (Some(h), Some(l)) = (hex(b[i + 1]), hex(b[i + 2])) {
                out.push((h * 16 + l) as u8);
                i += 3;
                continue;
            }
        }
        out.push(b[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Aegisub'ın uuencode biçimi (6 bit + 33, satır sonu yok).
pub fn uuencode(data: &[u8]) -> String {
    let mut out = String::with_capacity(data.len() * 4 / 3 + 4);
    let mut pos = 0;
    while pos < data.len() {
        let n = (data.len() - pos).min(3);
        let mut src = [0u8; 3];
        src[..n].copy_from_slice(&data[pos..pos + n]);
        let dst = [src[0] >> 2, ((src[0] & 0x3) << 4) | (src[1] >> 4), ((src[1] & 0xF) << 2) | (src[2] >> 6), src[2] & 0x3F];
        for d in dst.iter().take(n + 1) {
            out.push((d + 33) as char);
        }
        pos += 3;
    }
    out
}

pub fn uudecode(s: &str) -> Vec<u8> {
    let chars: Vec<u8> = s.bytes().filter(|b| !b.is_ascii_whitespace()).collect();
    let mut out = Vec::with_capacity(chars.len() * 3 / 4);
    let mut pos = 0;
    while pos + 1 < chars.len() {
        let n = (chars.len() - pos).min(4);
        let mut src = [0u8; 4];
        for i in 0..n {
            src[i] = chars[pos + i].wrapping_sub(33);
        }
        out.push((src[0] << 2) | (src[1] >> 4));
        if n > 2 {
            out.push(((src[1] & 0xF) << 4) | (src[2] >> 2));
        }
        if n > 3 {
            out.push(((src[2] & 0x3) << 6) | src[3]);
        }
        pos += 4;
    }
    out
}

/// Değeri Aegisub'ın yazdığı gibi kodlar (hangisi kısaysa).
pub fn encode_value(v: &str) -> String {
    let enc = inline_encode(v);
    if 4 * v.len() < 3 * enc.len() {
        format!("u{}", uuencode(v.as_bytes()))
    } else {
        format!("e{}", enc)
    }
}

pub fn decode_value(v: &str) -> String {
    if let Some(r) = v.strip_prefix('e') {
        inline_decode(r)
    } else if let Some(r) = v.strip_prefix('u') {
        String::from_utf8_lossy(&uudecode(r)).into_owned()
    } else {
        v.to_string()
    }
}

/// `Data: kimlik,anahtar,değer` satırını çözer.
pub fn parse_data_line(raw: &str) -> Option<(u32, String, String)> {
    let (k, rest) = raw.split_once(':')?;
    if !k.trim().eq_ignore_ascii_case("Data") {
        return None;
    }
    let mut it = rest.trim_start().splitn(3, ',');
    let id = it.next()?.trim().parse().ok()?;
    let key = inline_decode(it.next()?);
    let value = decode_value(it.next().unwrap_or(""));
    Some((id, key, value))
}

pub fn data_line(id: u32, key: &str, value: &str) -> String {
    format!("Data: {},{},{}", id, inline_encode(key), encode_value(value))
}

impl Script {
    /// Tüm Extradata girdileri: kimlik → (anahtar, değer).
    pub fn extradata(&self) -> BTreeMap<u32, (String, String)> {
        let mut m = BTreeMap::new();
        for e in &self.entries {
            if let Entry::Unknown { section, raw } = e {
                if section == SEC_EXTRADATA {
                    if let Some((id, k, v)) = parse_data_line(raw) {
                        m.insert(id, (k, v));
                    }
                }
            }
        }
        m
    }

    /// Kimlik listesini anahtar → değer tablosuna çevirir (sonraki kimlik öncekini ezer).
    pub fn extra_values(&self, ids: &[u32]) -> BTreeMap<String, String> {
        let mut out = BTreeMap::new();
        if ids.is_empty() {
            return out;
        }
        let all = self.extradata();
        for id in ids {
            if let Some((k, v)) = all.get(id) {
                out.insert(k.clone(), v.clone());
            }
        }
        out
    }

    /// Anahtar/değer çiftleri için kimlik listesi verir; olmayanları bölüme ekler.
    pub fn intern_extra(&mut self, values: &BTreeMap<String, String>) -> Vec<u32> {
        if values.is_empty() {
            return Vec::new();
        }
        let all = self.extradata();
        let mut next = all.keys().next_back().copied().unwrap_or(0) + 1;
        let mut ids = Vec::with_capacity(values.len());
        for (k, v) in values {
            if let Some((&id, _)) = all.iter().find(|(_, (ek, ev))| ek == k && ev == v) {
                ids.push(id);
                continue;
            }
            let line = data_line(next, k, v);
            let pos = self.entries.iter().rposition(|e| e.section() == SEC_EXTRADATA).map(|i| i + 1).unwrap_or(self.entries.len());
            self.entries.insert(pos, Entry::Unknown { section: SEC_EXTRADATA.to_string(), raw: line });
            ids.push(next);
            next += 1;
        }
        ids
    }

    /// Hiçbir satırın başvurmadığı Extradata girdilerini siler (Aegisub kayıtta yapar).
    pub fn clean_extradata(&mut self) -> usize {
        let used: std::collections::HashSet<u32> = self.dialogues().flat_map(|d| d.extra.iter().copied()).collect();
        let before = self.entries.len();
        self.entries.retain(|e| match e {
            Entry::Unknown { section, raw } if section == SEC_EXTRADATA => match parse_data_line(raw) {
                Some((id, _, _)) => used.contains(&id),
                None => !raw.trim().is_empty(),
            },
            _ => true,
        });
        before - self.entries.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prefix_split() {
        assert_eq!(split_prefix("{=1=22}Merhaba"), (vec![1, 22], "Merhaba"));
        assert_eq!(split_prefix("{=3}{\\i1}a"), (vec![3], "{\\i1}a"));
        assert_eq!(split_prefix("{=x}a"), (vec![], "{=x}a"));
        assert_eq!(split_prefix("{=}a"), (vec![], "{=}a"));
        assert_eq!(split_prefix("{\\i1}a"), (vec![], "{\\i1}a"));
        assert_eq!(prefix(&[4, 5]), "{=4=5}");
    }

    #[test]
    fn value_codecs() {
        for s in ["", "a", "ab", "abc", "abcd", "x,y:z|#\n", "ğüşİı çok, virgül", "{\"a\":[1,2,3]}"] {
            assert_eq!(decode_value(&encode_value(s)), s, "{s}");
            assert_eq!(String::from_utf8(uudecode(&uuencode(s.as_bytes()))).unwrap(), s);
            assert_eq!(inline_decode(&inline_encode(s)), s);
        }
        assert_eq!(inline_encode("a,b"), "a#2Cb");
        // 0x61 0x62 0x63 → 6 bitlik 24,22,9,35 → +33
        assert_eq!(uuencode(b"a"), "91");
        assert_eq!(uuencode(b"abc"), "97*D");
        assert_eq!(uudecode("97*D"), b"abc");
    }

    #[test]
    fn script_roundtrip_and_intern() {
        let src = "[Script Info]\nScriptType: v4.00+\n\n[Events]\nFormat: Layer, Start, End, Style, Name, MarginL, MarginR, MarginV, Effect, Text\nDialogue: 0,0:00:00.00,0:00:01.00,Default,,0,0,0,,{=1=2}Selam\n\n[Aegisub Extradata]\nData: 1,a.json,e{\"x\"#3A1}\nData: 2,b,u97*D\nData: 9,eski,ehiç\n";
        let mut s = Script::parse(src);
        let d = s.dialogues().next().unwrap().clone();
        assert_eq!(d.text, "Selam");
        assert_eq!(d.extra, vec![1, 2]);
        let vals = s.extra_values(&d.extra);
        assert_eq!(vals["a.json"], "{\"x\":1}");
        assert_eq!(vals["b"], "abc");
        let out = s.to_ass_string();
        assert!(out.contains("{=1=2}Selam"));
        assert!(out.contains("Data: 2,b,u97*D"));
        // aynı değer yeniden kullanılır, yenisi sona eklenir
        let mut m = BTreeMap::new();
        m.insert("b".to_string(), "abc".to_string());
        m.insert("c".to_string(), "yeni, değer".to_string());
        let ids = s.intern_extra(&m);
        assert_eq!(ids, vec![2, 10]);
        assert!(s.to_ass_string().contains("Data: 10,c,eyeni#2C değer"));
        assert_eq!(s.clean_extradata(), 2); // 9 ve 10 kullanılmıyor
        assert!(!s.to_ass_string().contains("Data: 9"));
    }

    #[test]
    fn intern_creates_section() {
        let mut s = Script::new_default();
        let mut m = BTreeMap::new();
        m.insert("k".to_string(), "v".to_string());
        assert_eq!(s.intern_extra(&m), vec![1]);
        let t = s.to_ass_string().replace("\r\n", "\n");
        assert!(t.contains("[Aegisub Extradata]\nData: 1,k,ev"), "{t}");
    }
}
