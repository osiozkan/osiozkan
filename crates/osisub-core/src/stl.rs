//! EBU STL (EBU Tech 3264) okuma/yazma — yayıncıların istediği ikili teletext altyazı biçimi.
//!
//! 1024 baytlık GSI başlığı + 128 baytlık TTI blokları. Metin ISO 6937 (aksanlar ön ekli
//! birleştirici baytlar: `ş` = 0xCB `s`, `ğ` = 0xC6 `g`, `İ` = 0xC7 `I`, `ı` = 0xF5),
//! 0x8A satır sonu, 0x80/0x81 italik, 0x82/0x83 altı çizili. Uzantı blokları birleştirilir,
//! yorum blokları atlanır, programın başlangıç zaman kodu (ör. 10:00:00:00) düşülür.
use crate::ass::{Dialogue, Script};

/// (karakter, ISO 6937 aksan baytı, temel harf) — Python unicodedata ile üretildi.
const COMPOSED: &[(char, u8, u8)] = &[
    ('\u{C0}', 0xC1, b'A'),
    ('\u{C1}', 0xC2, b'A'),
    ('\u{C2}', 0xC3, b'A'),
    ('\u{C3}', 0xC4, b'A'),
    ('\u{C4}', 0xC8, b'A'),
    ('\u{C5}', 0xCA, b'A'),
    ('\u{C7}', 0xCB, b'C'),
    ('\u{C8}', 0xC1, b'E'),
    ('\u{C9}', 0xC2, b'E'),
    ('\u{CA}', 0xC3, b'E'),
    ('\u{CB}', 0xC8, b'E'),
    ('\u{CC}', 0xC1, b'I'),
    ('\u{CD}', 0xC2, b'I'),
    ('\u{CE}', 0xC3, b'I'),
    ('\u{CF}', 0xC8, b'I'),
    ('\u{D1}', 0xC4, b'N'),
    ('\u{D2}', 0xC1, b'O'),
    ('\u{D3}', 0xC2, b'O'),
    ('\u{D4}', 0xC3, b'O'),
    ('\u{D5}', 0xC4, b'O'),
    ('\u{D6}', 0xC8, b'O'),
    ('\u{D9}', 0xC1, b'U'),
    ('\u{DA}', 0xC2, b'U'),
    ('\u{DB}', 0xC3, b'U'),
    ('\u{DC}', 0xC8, b'U'),
    ('\u{DD}', 0xC2, b'Y'),
    ('\u{E0}', 0xC1, b'a'),
    ('\u{E1}', 0xC2, b'a'),
    ('\u{E2}', 0xC3, b'a'),
    ('\u{E3}', 0xC4, b'a'),
    ('\u{E4}', 0xC8, b'a'),
    ('\u{E5}', 0xCA, b'a'),
    ('\u{E7}', 0xCB, b'c'),
    ('\u{E8}', 0xC1, b'e'),
    ('\u{E9}', 0xC2, b'e'),
    ('\u{EA}', 0xC3, b'e'),
    ('\u{EB}', 0xC8, b'e'),
    ('\u{EC}', 0xC1, b'i'),
    ('\u{ED}', 0xC2, b'i'),
    ('\u{EE}', 0xC3, b'i'),
    ('\u{EF}', 0xC8, b'i'),
    ('\u{F1}', 0xC4, b'n'),
    ('\u{F2}', 0xC1, b'o'),
    ('\u{F3}', 0xC2, b'o'),
    ('\u{F4}', 0xC3, b'o'),
    ('\u{F5}', 0xC4, b'o'),
    ('\u{F6}', 0xC8, b'o'),
    ('\u{F9}', 0xC1, b'u'),
    ('\u{FA}', 0xC2, b'u'),
    ('\u{FB}', 0xC3, b'u'),
    ('\u{FC}', 0xC8, b'u'),
    ('\u{FD}', 0xC2, b'y'),
    ('\u{FF}', 0xC8, b'y'),
    ('\u{100}', 0xC5, b'A'),
    ('\u{101}', 0xC5, b'a'),
    ('\u{102}', 0xC6, b'A'),
    ('\u{103}', 0xC6, b'a'),
    ('\u{104}', 0xCE, b'A'),
    ('\u{105}', 0xCE, b'a'),
    ('\u{106}', 0xC2, b'C'),
    ('\u{107}', 0xC2, b'c'),
    ('\u{108}', 0xC3, b'C'),
    ('\u{109}', 0xC3, b'c'),
    ('\u{10A}', 0xC7, b'C'),
    ('\u{10B}', 0xC7, b'c'),
    ('\u{10C}', 0xCF, b'C'),
    ('\u{10D}', 0xCF, b'c'),
    ('\u{10E}', 0xCF, b'D'),
    ('\u{10F}', 0xCF, b'd'),
    ('\u{112}', 0xC5, b'E'),
    ('\u{113}', 0xC5, b'e'),
    ('\u{114}', 0xC6, b'E'),
    ('\u{115}', 0xC6, b'e'),
    ('\u{116}', 0xC7, b'E'),
    ('\u{117}', 0xC7, b'e'),
    ('\u{118}', 0xCE, b'E'),
    ('\u{119}', 0xCE, b'e'),
    ('\u{11A}', 0xCF, b'E'),
    ('\u{11B}', 0xCF, b'e'),
    ('\u{11C}', 0xC3, b'G'),
    ('\u{11D}', 0xC3, b'g'),
    ('\u{11E}', 0xC6, b'G'),
    ('\u{11F}', 0xC6, b'g'),
    ('\u{120}', 0xC7, b'G'),
    ('\u{121}', 0xC7, b'g'),
    ('\u{122}', 0xCB, b'G'),
    ('\u{123}', 0xCB, b'g'),
    ('\u{124}', 0xC3, b'H'),
    ('\u{125}', 0xC3, b'h'),
    ('\u{128}', 0xC4, b'I'),
    ('\u{129}', 0xC4, b'i'),
    ('\u{12A}', 0xC5, b'I'),
    ('\u{12B}', 0xC5, b'i'),
    ('\u{12C}', 0xC6, b'I'),
    ('\u{12D}', 0xC6, b'i'),
    ('\u{12E}', 0xCE, b'I'),
    ('\u{12F}', 0xCE, b'i'),
    ('\u{130}', 0xC7, b'I'),
    ('\u{134}', 0xC3, b'J'),
    ('\u{135}', 0xC3, b'j'),
    ('\u{136}', 0xCB, b'K'),
    ('\u{137}', 0xCB, b'k'),
    ('\u{139}', 0xC2, b'L'),
    ('\u{13A}', 0xC2, b'l'),
    ('\u{13B}', 0xCB, b'L'),
    ('\u{13C}', 0xCB, b'l'),
    ('\u{13D}', 0xCF, b'L'),
    ('\u{13E}', 0xCF, b'l'),
    ('\u{143}', 0xC2, b'N'),
    ('\u{144}', 0xC2, b'n'),
    ('\u{145}', 0xCB, b'N'),
    ('\u{146}', 0xCB, b'n'),
    ('\u{147}', 0xCF, b'N'),
    ('\u{148}', 0xCF, b'n'),
    ('\u{14C}', 0xC5, b'O'),
    ('\u{14D}', 0xC5, b'o'),
    ('\u{14E}', 0xC6, b'O'),
    ('\u{14F}', 0xC6, b'o'),
    ('\u{150}', 0xCD, b'O'),
    ('\u{151}', 0xCD, b'o'),
    ('\u{154}', 0xC2, b'R'),
    ('\u{155}', 0xC2, b'r'),
    ('\u{156}', 0xCB, b'R'),
    ('\u{157}', 0xCB, b'r'),
    ('\u{158}', 0xCF, b'R'),
    ('\u{159}', 0xCF, b'r'),
    ('\u{15A}', 0xC2, b'S'),
    ('\u{15B}', 0xC2, b's'),
    ('\u{15C}', 0xC3, b'S'),
    ('\u{15D}', 0xC3, b's'),
    ('\u{15E}', 0xCB, b'S'),
    ('\u{15F}', 0xCB, b's'),
    ('\u{160}', 0xCF, b'S'),
    ('\u{161}', 0xCF, b's'),
    ('\u{162}', 0xCB, b'T'),
    ('\u{163}', 0xCB, b't'),
    ('\u{164}', 0xCF, b'T'),
    ('\u{165}', 0xCF, b't'),
    ('\u{168}', 0xC4, b'U'),
    ('\u{169}', 0xC4, b'u'),
    ('\u{16A}', 0xC5, b'U'),
    ('\u{16B}', 0xC5, b'u'),
    ('\u{16C}', 0xC6, b'U'),
    ('\u{16D}', 0xC6, b'u'),
    ('\u{16E}', 0xCA, b'U'),
    ('\u{16F}', 0xCA, b'u'),
    ('\u{170}', 0xCD, b'U'),
    ('\u{171}', 0xCD, b'u'),
    ('\u{172}', 0xCE, b'U'),
    ('\u{173}', 0xCE, b'u'),
    ('\u{174}', 0xC3, b'W'),
    ('\u{175}', 0xC3, b'w'),
    ('\u{176}', 0xC3, b'Y'),
    ('\u{177}', 0xC3, b'y'),
    ('\u{178}', 0xC8, b'Y'),
    ('\u{179}', 0xC2, b'Z'),
    ('\u{17A}', 0xC2, b'z'),
    ('\u{17B}', 0xC7, b'Z'),
    ('\u{17C}', 0xC7, b'z'),
    ('\u{17D}', 0xCF, b'Z'),
    ('\u{17E}', 0xCF, b'z'),
    ('\u{1CD}', 0xCF, b'A'),
    ('\u{1CE}', 0xCF, b'a'),
    ('\u{1CF}', 0xCF, b'I'),
    ('\u{1D0}', 0xCF, b'i'),
    ('\u{1D1}', 0xCF, b'O'),
    ('\u{1D2}', 0xCF, b'o'),
    ('\u{1D3}', 0xCF, b'U'),
    ('\u{1D4}', 0xCF, b'u'),
    ('\u{1E6}', 0xCF, b'G'),
    ('\u{1E7}', 0xCF, b'g'),
    ('\u{1E8}', 0xCF, b'K'),
    ('\u{1E9}', 0xCF, b'k'),
    ('\u{1EA}', 0xCE, b'O'),
    ('\u{1EB}', 0xCE, b'o'),
    ('\u{1F0}', 0xCF, b'j'),
    ('\u{1F4}', 0xC2, b'G'),
    ('\u{1F5}', 0xC2, b'g'),
    ('\u{1F8}', 0xC1, b'N'),
    ('\u{1F9}', 0xC1, b'n'),
    ('\u{21E}', 0xCF, b'H'),
    ('\u{21F}', 0xCF, b'h'),
    ('\u{226}', 0xC7, b'A'),
    ('\u{227}', 0xC7, b'a'),
    ('\u{228}', 0xCB, b'E'),
    ('\u{229}', 0xCB, b'e'),
    ('\u{22E}', 0xC7, b'O'),
    ('\u{22F}', 0xC7, b'o'),
    ('\u{232}', 0xC5, b'Y'),
    ('\u{233}', 0xC5, b'y'),
    ('\u{1E02}', 0xC7, b'B'),
    ('\u{1E03}', 0xC7, b'b'),
    ('\u{1E0A}', 0xC7, b'D'),
    ('\u{1E0B}', 0xC7, b'd'),
    ('\u{1E10}', 0xCB, b'D'),
    ('\u{1E11}', 0xCB, b'd'),
    ('\u{1E1E}', 0xC7, b'F'),
    ('\u{1E1F}', 0xC7, b'f'),
    ('\u{1E20}', 0xC5, b'G'),
    ('\u{1E21}', 0xC5, b'g'),
    ('\u{1E22}', 0xC7, b'H'),
    ('\u{1E23}', 0xC7, b'h'),
    ('\u{1E26}', 0xC8, b'H'),
    ('\u{1E27}', 0xC8, b'h'),
    ('\u{1E28}', 0xCB, b'H'),
    ('\u{1E29}', 0xCB, b'h'),
    ('\u{1E30}', 0xC2, b'K'),
    ('\u{1E31}', 0xC2, b'k'),
    ('\u{1E3E}', 0xC2, b'M'),
    ('\u{1E3F}', 0xC2, b'm'),
    ('\u{1E40}', 0xC7, b'M'),
    ('\u{1E41}', 0xC7, b'm'),
    ('\u{1E44}', 0xC7, b'N'),
    ('\u{1E45}', 0xC7, b'n'),
    ('\u{1E54}', 0xC2, b'P'),
    ('\u{1E55}', 0xC2, b'p'),
    ('\u{1E56}', 0xC7, b'P'),
    ('\u{1E57}', 0xC7, b'p'),
    ('\u{1E58}', 0xC7, b'R'),
    ('\u{1E59}', 0xC7, b'r'),
    ('\u{1E60}', 0xC7, b'S'),
    ('\u{1E61}', 0xC7, b's'),
    ('\u{1E6A}', 0xC7, b'T'),
    ('\u{1E6B}', 0xC7, b't'),
    ('\u{1E7C}', 0xC4, b'V'),
    ('\u{1E7D}', 0xC4, b'v'),
    ('\u{1E80}', 0xC1, b'W'),
    ('\u{1E81}', 0xC1, b'w'),
    ('\u{1E82}', 0xC2, b'W'),
    ('\u{1E83}', 0xC2, b'w'),
    ('\u{1E84}', 0xC8, b'W'),
    ('\u{1E85}', 0xC8, b'w'),
    ('\u{1E86}', 0xC7, b'W'),
    ('\u{1E87}', 0xC7, b'w'),
    ('\u{1E8A}', 0xC7, b'X'),
    ('\u{1E8B}', 0xC7, b'x'),
    ('\u{1E8C}', 0xC8, b'X'),
    ('\u{1E8D}', 0xC8, b'x'),
    ('\u{1E8E}', 0xC7, b'Y'),
    ('\u{1E8F}', 0xC7, b'y'),
    ('\u{1E90}', 0xC3, b'Z'),
    ('\u{1E91}', 0xC3, b'z'),
    ('\u{1E97}', 0xC8, b't'),
    ('\u{1E98}', 0xCA, b'w'),
    ('\u{1E99}', 0xCA, b'y'),
    ('\u{1EBC}', 0xC4, b'E'),
    ('\u{1EBD}', 0xC4, b'e'),
    ('\u{1EF2}', 0xC1, b'Y'),
    ('\u{1EF3}', 0xC1, b'y'),
    ('\u{1EF8}', 0xC4, b'Y'),
    ('\u{1EF9}', 0xC4, b'y'),
];

/// ISO 6937'nin 0xA0–0xFF bölgesindeki tek baytlık karakterler.
const SINGLE: &[(u8, char)] = &[
    (0xA0, '\u{a0}'), (0xA1, '¡'), (0xA2, '¢'), (0xA3, '£'), (0xA5, '¥'), (0xA7, '§'), (0xA8, '¤'), (0xA9, '‘'),
    (0xAA, '“'), (0xAB, '«'), (0xAC, '←'), (0xAD, '↑'), (0xAE, '→'), (0xAF, '↓'), (0xB0, '°'), (0xB1, '±'),
    (0xB2, '²'), (0xB3, '³'), (0xB4, '×'), (0xB5, 'µ'), (0xB6, '¶'), (0xB7, '·'), (0xB8, '÷'), (0xB9, '’'),
    (0xBA, '”'), (0xBB, '»'), (0xBC, '¼'), (0xBD, '½'), (0xBE, '¾'), (0xBF, '¿'), (0xD0, '―'), (0xD1, '¹'),
    (0xD2, '®'), (0xD3, '©'), (0xD4, '™'), (0xD5, '♪'), (0xD6, '¬'), (0xD7, '¦'), (0xDC, '⅛'), (0xDD, '⅜'),
    (0xDE, '⅝'), (0xDF, '⅞'), (0xE0, 'Ω'), (0xE1, 'Æ'), (0xE2, 'Đ'), (0xE3, 'ª'), (0xE4, 'Ħ'), (0xE6, 'Ĳ'),
    (0xE7, 'Ŀ'), (0xE8, 'Ł'), (0xE9, 'Ø'), (0xEA, 'Œ'), (0xEB, 'º'), (0xEC, 'Þ'), (0xED, 'Ŧ'), (0xEE, 'Ŋ'),
    (0xEF, 'ŉ'), (0xF0, 'ĸ'), (0xF1, 'æ'), (0xF2, 'đ'), (0xF3, 'ð'), (0xF4, 'ħ'), (0xF5, 'ı'), (0xF6, 'ĳ'),
    (0xF7, 'ŀ'), (0xF8, 'ł'), (0xF9, 'ø'), (0xFA, 'œ'), (0xFB, 'ß'), (0xFC, 'þ'), (0xFD, 'ŧ'), (0xFE, 'ŋ'),
];

fn encode_char(c: char, out: &mut Vec<u8>) {
    match c {
        ' '..='~' => out.push(c as u8),
        '\u{a0}' => out.push(b' '),
        '…' => out.extend_from_slice(b"..."),
        '–' | '—' => out.push(b'-'),
        _ => {
            if let Some(&(b, _)) = SINGLE.iter().find(|(_, ch)| *ch == c) {
                out.push(b);
            } else if let Some(&(_, m, base)) = COMPOSED.iter().find(|(ch, _, _)| *ch == c) {
                out.push(m);
                out.push(base);
            } else {
                out.push(b'?');
            }
        }
    }
}

/// TF baytlarını ASS metnine çevirir (biçim baytları → etiketler).
fn decode_text(b: &[u8]) -> String {
    let mut s = String::new();
    let (mut ital, mut und) = (false, false);
    let mut i = 0;
    while i < b.len() {
        let c = b[i];
        i += 1;
        match c {
            0x8F => break,
            0x8A => {
                if !s.ends_with("\\N") && !s.is_empty() {
                    s.push_str("\\N");
                }
            }
            0x80 if !ital => {
                s.push_str("{\\i1}");
                ital = true;
            }
            0x81 if ital => {
                s.push_str("{\\i0}");
                ital = false;
            }
            0x82 if !und => {
                s.push_str("{\\u1}");
                und = true;
            }
            0x83 if und => {
                s.push_str("{\\u0}");
                und = false;
            }
            0x00..=0x1F | 0x7F..=0x9F => {
                // teletext renk/boyut/kutu denetimleri: boşluk gibi davranır
                if c <= 0x1F && !s.is_empty() && !s.ends_with(' ') && !s.ends_with("\\N") && !s.ends_with('}') {
                    s.push(' ');
                }
            }
            0x20..=0x7E => s.push(c as char),
            0xC1..=0xCF => {
                let base = b.get(i).copied().unwrap_or(b' ');
                if let Some(&(ch, _, _)) = COMPOSED.iter().find(|(_, m, bs)| *m == c && *bs == base) {
                    s.push(ch);
                    i += 1;
                } else if (0x41..=0x7A).contains(&base) {
                    s.push(base as char);
                    i += 1;
                }
            }
            _ => {
                if let Some(&(_, ch)) = SINGLE.iter().find(|(x, _)| *x == c) {
                    s.push(ch);
                }
            }
        }
    }
    if ital {
        s.push_str("{\\i0}");
    }
    if und {
        s.push_str("{\\u0}");
    }
    s.trim_end_matches("\\N").trim().replace(" \\N", "\\N").replace("\\N ", "\\N")
}

fn ascii_field(b: &[u8]) -> String {
    String::from_utf8_lossy(b).trim().to_string()
}

fn fps_of(dfc: &str) -> f64 {
    if dfc.starts_with("STL30") {
        30.0
    } else {
        25.0
    }
}

fn tc_ms(t: &[u8], fps: f64) -> i64 {
    let (h, m, s, f) = (t[0] as i64, t[1] as i64, t[2] as i64, t[3] as i64);
    (h * 3600 + m * 60 + s) * 1000 + ((f as f64) * 1000.0 / fps).round() as i64
}

fn ascii_tc_ms(t: &str, fps: f64) -> Option<i64> {
    if t.len() != 8 || !t.bytes().all(|c| c.is_ascii_digit()) {
        return None;
    }
    let n = |i: usize| t[i..i + 2].parse::<i64>().unwrap_or(0);
    Some((n(0) * 3600 + n(2) * 60 + n(4)) * 1000 + ((n(6) as f64) * 1000.0 / fps).round() as i64)
}

/// EBU STL dosyası mı (GSI'deki "STL25.01"/"STL30.01" imzası)?
pub fn is_stl(b: &[u8]) -> bool {
    b.len() >= 1024 && b[3..6] == *b"STL" && b[8] == b'.'
}

pub fn parse(b: &[u8]) -> Result<Script, String> {
    if !is_stl(b) {
        return Err("EBU STL değil (GSI imzası yok)".into());
    }
    let gsi = &b[..1024];
    let fps = fps_of(&ascii_field(&gsi[3..11]));
    let mnr: u8 = ascii_field(&gsi[253..255]).parse().unwrap_or(23);
    let tcp = ascii_tc_ms(&ascii_field(&gsi[256..264]), fps).unwrap_or(0);
    let open_sub = gsi[11] == b'0';
    // (alt yazı no) → (başlangıç, bitiş, VP, JC, metin baytları)
    let mut cues: Vec<(u16, i64, i64, u8, u8, Vec<u8>)> = Vec::new();
    // Önceki blok aynı altyazının devamını bekliyor mu (EBN ≠ FF)?
    let mut open = false;
    for blk in b[1024..].chunks_exact(128) {
        let sn = u16::from_le_bytes([blk[1], blk[2]]);
        let ebn = blk[3];
        if ebn == 0xFE || blk[15] != 0 {
            continue; // kullanıcı verisi / yorum
        }
        let tf = &blk[16..128];
        // Uzantı bloğunun 0x8F dolgusu metnin sonudur; parçalar birleştirilmeden önce kırpılır.
        let used = tf.iter().position(|&c| c == 0x8F).unwrap_or(tf.len());
        match cues.last_mut() {
            Some(last) if open && last.0 == sn => last.5.extend_from_slice(&tf[..used]),
            _ => cues.push((sn, tc_ms(&blk[5..9], fps), tc_ms(&blk[9..13], fps), blk[13], blk[14], tf[..used].to_vec())),
        }
        open = ebn != 0xFF;
    }
    let offset = if tcp > 0 && cues.iter().all(|c| c.1 >= tcp) { tcp } else { 0 };
    let mut out = Vec::with_capacity(cues.len());
    for (_, st, en, vp, jc, tf) in cues {
        let text = decode_text(&tf);
        if text.is_empty() {
            continue;
        }
        let top = if open_sub { vp < 50 } else { vp > 0 && (vp as u32) * 2 < mnr as u32 };
        let an = match (top, jc) {
            (true, 1) => Some(7),
            (true, 3) => Some(9),
            (true, _) => Some(8),
            (false, 1) => Some(1),
            (false, 3) => Some(3),
            _ => None,
        };
        let text = match an {
            Some(a) => format!("{{\\an{}}}{}", a, text),
            None => text,
        };
        out.push(Dialogue { start_time: st - offset, end_time: (en - offset).max(st - offset), text, ..Default::default() });
    }
    let mut sc = Script::from_dialogues(out);
    let title = ascii_field(&gsi[16..48]);
    if !title.is_empty() {
        sc.set_info("Title", &title);
    }
    Ok(sc)
}

fn pad(s: &str, n: usize) -> Vec<u8> {
    let mut v: Vec<u8> = Vec::new();
    for c in s.chars() {
        if c.is_ascii() && !c.is_ascii_control() {
            v.push(c as u8);
        }
    }
    v.resize(n, b' ');
    v.truncate(n);
    v
}

fn tc_bytes(ms: i64, fps: f64) -> [u8; 4] {
    let total_frames = ((ms.max(0) as f64) * fps / 1000.0).round() as i64;
    let f = fps as i64;
    let secs = total_frames / f;
    [(secs / 3600).min(255) as u8, (secs / 60 % 60) as u8, (secs % 60) as u8, (total_frames % f) as u8]
}

/// Satırı TF baytlarına çevirir (biçim baytlarıyla).
fn encode_text(text: &str) -> (Vec<u8>, usize) {
    let mut out = Vec::new();
    let (mut ital, mut und) = (false, false);
    let mut lines = 1;
    for (t, i, _b, u) in crate::ttml::styled_runs(text) {
        if i != ital {
            out.push(if i { 0x80 } else { 0x81 });
            ital = i;
        }
        if u != und {
            out.push(if u { 0x82 } else { 0x83 });
            und = u;
        }
        for c in t.chars() {
            if c == '\n' {
                if ital {
                    out.push(0x81);
                }
                if und {
                    out.push(0x83);
                }
                out.push(0x8A);
                lines += 1;
                if ital {
                    out.push(0x80);
                }
                if und {
                    out.push(0x82);
                }
            } else {
                encode_char(c, &mut out);
            }
        }
    }
    if ital {
        out.push(0x81);
    }
    if und {
        out.push(0x83);
    }
    (out, lines)
}

/// EBU STL yazar (25 kare, Level-1 teletext, Latin karakter tablosu).
pub fn write(script: &Script, fps: f64) -> Vec<u8> {
    let fps = if (fps - 30.0).abs() < 1.0 { 30.0 } else { 25.0 };
    let lines: Vec<&Dialogue> = script.dialogues().filter(|d| !d.comment && !crate::tags::visible_text(&d.text).trim().is_empty()).collect();
    let mut ttis: Vec<[u8; 128]> = Vec::new();
    for (n, d) in lines.iter().enumerate() {
        let (tf, nlines) = encode_text(&d.text);
        let an = crate::tags::split(&d.text)
            .iter()
            .find_map(|p| match p {
                crate::tags::Part::Block(b) => b.find("\\an").and_then(|i| b[i + 3..].chars().next()).and_then(|c| c.to_digit(10)),
                _ => None,
            })
            .unwrap_or(2);
        let top = an >= 7;
        let jc = match an % 3 {
            1 => 1,
            0 => 3,
            _ => 2,
        };
        // Level-1 teletext: 23 satır, çift yükseklik varsayımıyla alttan 2'şer satır
        let vp = if top { 1 } else { (22 - 2 * (nlines.saturating_sub(1)) as i32).max(1) as u8 };
        let chunks: Vec<&[u8]> = if tf.is_empty() { vec![&[][..]] } else { tf.chunks(112).collect() };
        let last = chunks.len() - 1;
        for (k, ch) in chunks.into_iter().enumerate() {
            let mut b = [0x8Fu8; 128];
            b[0] = 0;
            b[1..3].copy_from_slice(&((n + 1) as u16).to_le_bytes());
            b[3] = if k == last { 0xFF } else { k as u8 };
            b[4] = 0;
            b[5..9].copy_from_slice(&tc_bytes(d.start_time, fps));
            b[9..13].copy_from_slice(&tc_bytes(d.end_time, fps));
            b[13] = vp;
            b[14] = jc;
            b[15] = 0;
            b[16..16 + ch.len()].copy_from_slice(ch);
            ttis.push(b);
        }
    }
    let mut g = Vec::with_capacity(1024);
    g.extend_from_slice(b"850");
    g.extend_from_slice(if fps == 30.0 { b"STL30.01" } else { b"STL25.01" });
    g.push(b'1'); // DSC: Level-1 teletext
    g.extend_from_slice(b"00"); // CCT: Latin
    g.extend_from_slice(b"00"); // LC
    let title = script.info("Title").unwrap_or("");
    g.extend(pad(title, 32)); // OPT
    g.extend(pad("", 32)); // OET
    g.extend(pad(title, 32)); // TPT
    g.extend(pad("", 32)); // TET
    g.extend(pad(script.info("Original Translation").unwrap_or(""), 32)); // TN
    g.extend(pad("", 32)); // TCD
    g.extend(pad("", 16)); // SLR
    g.extend(pad("000101", 6)); // CD
    g.extend(pad("000101", 6)); // RD
    g.extend(pad("00", 2)); // RN
    g.extend(pad(&format!("{:05}", ttis.len()), 5)); // TNB
    g.extend(pad(&format!("{:05}", lines.len()), 5)); // TNS
    g.extend(pad("001", 3)); // TNG
    g.extend(pad("40", 2)); // MNC
    g.extend(pad("23", 2)); // MNR
    g.push(b'1'); // TCS
    g.extend(pad("00000000", 8)); // TCP
    let first = lines.first().map(|d| tc_bytes(d.start_time, fps)).unwrap_or([0; 4]);
    g.extend(pad(&format!("{:02}{:02}{:02}{:02}", first[0], first[1], first[2], first[3]), 8)); // TCF
    g.push(b'1'); // TND
    g.push(b'1'); // DSN
    g.extend(pad("TUR", 3)); // CO
    g.resize(1024, b' ');
    let mut out = g;
    for t in ttis {
        out.extend_from_slice(&t);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn turkish_charset_roundtrip() {
        let s = "Çığ düştü, İŞÖĞÜ şöyle: ½ «tamam» ♪";
        let mut b = Vec::new();
        for c in s.chars() {
            encode_char(c, &mut b);
        }
        assert_eq!(&b[..3], &[0xCB, b'C', 0xF5]);
        assert_eq!(decode_text(&b), s);
    }

    #[test]
    fn write_parse_roundtrip() {
        let mut sc = Script::from_dialogues(vec![
            Dialogue { start_time: 1000, end_time: 2520, text: "Merhaba {\\i1}dünya{\\i0}\\Nikinci satır".into(), ..Default::default() },
            Dialogue { start_time: 3000, end_time: 4000, text: "{\\an8}Üstte yazı".into(), ..Default::default() },
            Dialogue { start_time: 5000, end_time: 6000, text: "Uzun ".repeat(40), ..Default::default() },
            Dialogue { start_time: 7000, end_time: 8000, text: "yorum".into(), comment: true, ..Default::default() },
        ]);
        sc.set_info("Title", "Bolum 1");
        let b = write(&sc, 25.0);
        assert!(is_stl(&b));
        assert_eq!(&b[3..11], b"STL25.01");
        // 3 satır; üçüncüsü 200 bayt → 2 TTI bloğu
        assert_eq!((b.len() - 1024) / 128, 4);
        assert_eq!(&b[238..243], b"00004");
        let back = parse(&b).unwrap();
        let d: Vec<_> = back.dialogues().collect();
        assert_eq!(d.len(), 3);
        assert_eq!(d[0].text, "Merhaba {\\i1}dünya{\\i0}\\Nikinci satır");
        assert_eq!((d[0].start_time, d[0].end_time), (1000, 2520));
        assert_eq!(d[1].text, "{\\an8}Üstte yazı");
        assert_eq!(d[2].text, "Uzun ".repeat(40).trim());
        assert_eq!(back.info("Title"), Some("Bolum 1"));
    }

    #[test]
    fn programme_start_offset_is_removed() {
        let sc = Script::from_dialogues(vec![Dialogue { start_time: 36_000_000 + 1000, end_time: 36_000_000 + 2000, text: "a".into(), ..Default::default() }]);
        let mut b = write(&sc, 25.0);
        b[256..264].copy_from_slice(b"10000000");
        let back = parse(&b).unwrap();
        assert_eq!(back.dialogues().next().unwrap().start_time, 1000);
        assert!(parse(b"kisa").is_err());
    }
}
