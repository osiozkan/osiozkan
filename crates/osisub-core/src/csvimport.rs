//! CSV/TSV içe aktarma, sütun eşlemeyle (Subtitle Edit Import CSV custom columns). Elektronik
//! tabloda çeviri yapan ekipler için: başlangıç, bitiş, metin, konuşan, stil sütunları seçilir.
use crate::ass::{Dialogue, Entry, Script};

/// RFC 4180 benzeri ayrıştırma: tırnaklı alanlar, `""` kaçışı, alan içi satır sonu.
pub fn parse_rows(text: &str, sep: char) -> Vec<Vec<String>> {
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let mut rows = vec![];
    let mut row: Vec<String> = vec![];
    let mut field = String::new();
    let mut quoted = false;
    let mut chars = text.chars().peekable();
    let mut any = false;
    while let Some(c) = chars.next() {
        any = true;
        if quoted {
            if c == '"' {
                if chars.peek() == Some(&'"') { chars.next(); field.push('"'); } else { quoted = false; }
            } else {
                field.push(c);
            }
            continue;
        }
        match c {
            '"' if field.is_empty() => quoted = true,
            c if c == sep => row.push(std::mem::take(&mut field)),
            '\r' => {}
            '\n' => {
                row.push(std::mem::take(&mut field));
                rows.push(std::mem::take(&mut row));
                any = false;
            }
            c => field.push(c),
        }
    }
    if any {
        row.push(field);
        rows.push(row);
    }
    rows.retain(|r| !(r.len() == 1 && r[0].trim().is_empty()));
    rows
}

/// En tutarlı ayraç (sekme, `;`, `,`, `|`): her aday tırnak kurallarıyla ayrıştırılır; ilk satırlarda
/// sütun sayısı en çok satırda aynı (ve >1) olan seçilir, eşitse daha çok sütun.
pub fn detect_sep(text: &str) -> char {
    let head: String = text.lines().take(60).collect::<Vec<_>>().join("\n");
    ['\t', ';', ',', '|']
        .into_iter()
        .max_by_key(|&c| {
            let rows = parse_rows(&head, c);
            let rows = &rows[..rows.len().min(20)];
            let mut counts: std::collections::HashMap<usize, usize> = Default::default();
            for r in rows { *counts.entry(r.len()).or_default() += 1; }
            let (cols, same) = counts.into_iter().max_by_key(|&(k, v)| (v, k)).unwrap_or((1, 0));
            (cols > 1, same, cols)
        })
        .unwrap_or(',')
}

/// Zaman: `0:00:01.50`, `00:00:01,500`, `01:02.5` ya da saniye (`61.5`).
pub fn parse_time(s: &str) -> Option<i64> {
    let s = s.trim();
    if s.contains(':') {
        return crate::time::parse_generic(s);
    }
    s.replace(',', ".").parse::<f64>().ok().filter(|v| v.is_finite() && *v >= 0.0).map(|v| (v * 1000.0).round() as i64)
}

/// Sütun dizinleri (0 tabanlı): başlangıç, bitiş, metin zorunlu; konuşan/stil isteğe bağlı.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Columns {
    pub start: usize,
    pub end: usize,
    pub text: usize,
    pub actor: Option<usize>,
    pub style: Option<usize>,
}

/// Başlık adlarından sütunları tahmin eder (Türkçe/İngilizce). Başlık sözcüklere ayrılır ("Start Time",
/// "end_time"); her alan için anahtarlar öncelik sırasıyla aranır.
pub fn guess_columns(header: &[String]) -> Option<Columns> {
    let words: Vec<Vec<String>> = header.iter().map(|h| crate::qc::tr_lower(h.trim()).split(|c: char| c.is_whitespace() || c == '_' || c == '-' || c == '.').filter(|w| !w.is_empty()).map(String::from).collect()).collect();
    let find = |keys: &[&str]| keys.iter().find_map(|k| words.iter().position(|ws| ws.iter().any(|w| w == k)));
    Some(Columns {
        start: find(&["start", "başlangıç", "baslangic", "begin", "in"])?,
        end: find(&["end", "bitiş", "bitis", "stop", "out"])?,
        text: find(&["text", "metin", "çeviri", "ceviri", "translation", "subtitle", "altyazı", "dialogue", "dialog"])?,
        actor: find(&["actor", "konuşan", "konusan", "speaker", "character", "karakter", "name"]),
        style: find(&["style", "stil"]),
    })
}

/// Satırları diyaloglara çevirir: (diyaloglar, atlanan satır sayısı). Metindeki satır sonları `\N` olur.
pub fn to_dialogues(rows: &[Vec<String>], header: bool, c: &Columns) -> (Vec<Dialogue>, usize) {
    let mut out = vec![];
    let mut skipped = 0;
    for r in rows.iter().skip(header as usize) {
        let get = |i: usize| r.get(i).map(|s| s.as_str()).unwrap_or("");
        match (parse_time(get(c.start)), parse_time(get(c.end))) {
            (Some(a), Some(b)) => {
                let mut d = Dialogue::default();
                d.start_time = a;
                d.end_time = b.max(a);
                d.text = get(c.text).trim().replace("\r\n", "\n").replace('\n', "\\N");
                if let Some(i) = c.actor { d.actor = get(i).trim().to_string(); }
                if let Some(i) = c.style { let st = get(i).trim(); if !st.is_empty() { d.style = st.to_string(); } }
                out.push(d);
            }
            _ => skipped += 1,
        }
    }
    (out, skipped)
}

/// Varsayılan betiğe eklenmiş yeni belge.
pub fn to_script(dialogues: Vec<Dialogue>) -> Script {
    let mut s = Script::default();
    s.entries.extend(dialogues.into_iter().map(Entry::Dialogue));
    s
}

/// Dosya açarken: ayraç sezilir, başlıktan sütunlar tahmin edilir; başlık yoksa ilk üç sütun
/// başlangıç/bitiş/metin sayılır (ilk satırın zamanları okunabiliyorsa).
pub fn load(text: &str) -> Script {
    let rows = parse_rows(text, detect_sep(text));
    let Some(first) = rows.first() else { return to_script(vec![]) };
    let (cols, header) = match guess_columns(first) {
        Some(c) => (c, true),
        None => (Columns { start: 0, end: 1, text: first.len().saturating_sub(1).max(2), actor: None, style: None }, false),
    };
    to_script(to_dialogues(&rows, header, &cols).0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn csv_roundtrip_like_spreadsheet() {
        let text = "\u{feff}Başlangıç;Bitiş;Karakter;Çeviri\r\n0:00:01.50;0:00:03.00;Saki;\"Merhaba; dünya\"\r\n4,5;6;Ken;\"İki\nsatır \"\"tırnak\"\"\"\r\nnot;bozuk;;x\r\n";
        assert_eq!(detect_sep(text), ';');
        let rows = parse_rows(text, ';');
        assert_eq!(rows.len(), 4);
        let c = guess_columns(&rows[0]).unwrap();
        assert_eq!(c, Columns { start: 0, end: 1, text: 3, actor: Some(2), style: None });
        let (d, skipped) = to_dialogues(&rows, true, &c);
        assert_eq!(skipped, 1);
        assert_eq!((d[0].start_time, d[0].end_time, d[0].actor.as_str(), d[0].text.as_str()), (1500, 3000, "Saki", "Merhaba; dünya"));
        assert_eq!((d[1].start_time, d[1].end_time, d[1].text.as_str()), (4500, 6000, "İki\\Nsatır \"tırnak\""));
        assert_eq!(detect_sep("a\tb\tc\n1\t2\t3"), '\t');
        assert!(guess_columns(&["x".into(), "y".into()]).is_none());
        let s = to_script(d);
        assert_eq!(s.dialogues().count(), 2);
    }

    #[test]
    fn own_csv_export_roundtrips() {
        let mut d = Dialogue::default();
        d.start_time = 1000;
        d.end_time = 2000;
        d.actor = "Saki".into();
        d.text = "Merhaba, dünya".into();
        let csv = crate::formats::write_csv(&Script::from_dialogues(vec![d]));
        let s = load(&csv);
        let v: Vec<_> = s.dialogues().collect();
        assert_eq!((v[0].start_time, v[0].end_time, v[0].actor.as_str(), v[0].text.as_str()), (1000, 2000, "Saki", "Merhaba, dünya"));
        // başlıksız: başlangıç, bitiş, metin
        let t = load("1.0\t2.5\tSelam\n3\t4\tNasılsın");
        assert_eq!(t.dialogues().map(|d| d.text.as_str()).collect::<Vec<_>>(), vec!["Selam", "Nasılsın"]);
    }

    #[test]
    fn excel_semicolon_and_headers() {
        let text = "Başlangıç;Bitiş;Çeviri\n1,5;2,5;\"Evet, hayır, şey,\nyani, tamam\"\n3;4;Bir, iki\n";
        assert_eq!(detect_sep(text), ';');
        let s = load(text);
        assert_eq!(s.dialogues().map(|d| (d.start_time, d.text.as_str())).collect::<Vec<_>>(), vec![(1500, "Evet, hayır, şey,\\Nyani, tamam"), (3000, "Bir, iki")]);
        let c = guess_columns(&["Line".into(), "Start".into(), "End".into(), "Text".into()]).unwrap();
        assert_eq!((c.start, c.end, c.text), (1, 2, 3));
        let t = load("#,Start Time,End Time,Text\n1,0:00:01.00,0:00:02.00,Merhaba");
        assert_eq!(t.dialogues().map(|d| (d.start_time, d.end_time, d.text.as_str())).collect::<Vec<_>>(), vec![(1000, 2000, "Merhaba")]);
    }
}
