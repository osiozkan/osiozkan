//! Satır bölme araçları (Aegisub "Split lines"): satır sonlarından (`\N`) ve karaoke hecelerinden.
use crate::ass::Dialogue;
use crate::tags::{self, Part};

/// Metni üst düzeydeki `\N`/`\n` sınırlarından parçalara ayırır (etiket blokları içinde değil).
fn split_text(text: &str) -> Vec<String> {
    let mut parts = vec![String::new()];
    for p in tags::split(text) {
        match p {
            Part::Block(b) => {
                let last = parts.last_mut().unwrap();
                last.push('{');
                last.push_str(b);
                last.push('}');
            }
            Part::Text(t) => {
                let t = t.replace("\\n", "\\N");
                let mut it = t.split("\\N");
                if let Some(first) = it.next() {
                    parts.last_mut().unwrap().push_str(first);
                }
                for rest in it {
                    parts.push(rest.to_string());
                }
            }
        }
    }
    parts
}

/// Baştaki etiket bloğu (varsa) — her parçaya taşınır (konum, stil etiketleri).
fn leading_block(text: &str) -> Option<String> {
    match tags::split(text).first() {
        Some(Part::Block(b)) => Some(format!("{{{}}}", b)),
        _ => None,
    }
}

/// Parçanın sonunda italik/kalın/altı çizili açık mı (sonraki parçaya taşımak için).
fn open_styles(text: &str) -> String {
    let (mut i, mut b, mut u) = (false, false, false);
    for p in tags::split(text) {
        if let Part::Block(blk) = p {
            for t in blk.split('\\').skip(1) {
                let num = |n: &str| t.strip_prefix(n).filter(|r| !r.is_empty() && r.chars().all(|c| c.is_ascii_digit())).and_then(|r| r.parse::<u32>().ok());
                if let Some(v) = num("i") {
                    i = v > 0;
                } else if let Some(v) = num("b") {
                    b = v == 1 || v >= 600;
                } else if let Some(v) = num("u") {
                    u = v > 0;
                } else if t == "r" {
                    i = false;
                    b = false;
                    u = false;
                }
            }
        }
    }
    let mut s = String::new();
    if i {
        s.push_str("\\i1");
    }
    if b {
        s.push_str("\\b1");
    }
    if u {
        s.push_str("\\u1");
    }
    s
}

/// Satırı `\N` noktalarından ayrı olaylara böler. `keep_times`: her parça aynı zamanı alır
/// (üst/alt aynı anda); değilse süre görünür karakter sayısına göre paylaşılır. Bölünecek bir şey
/// yoksa tek öğeli liste döner. Boş parçalar atlanır.
pub fn split_at_breaks(d: &Dialogue, keep_times: bool) -> Vec<Dialogue> {
    let parts = split_text(&d.text);
    if parts.len() < 2 {
        return vec![d.clone()];
    }
    let lead = leading_block(&d.text).unwrap_or_default();
    let mut texts: Vec<String> = Vec::new();
    let mut carry = String::new();
    for (k, p) in parts.iter().enumerate() {
        let body = if k == 0 {
            p.clone()
        } else {
            let mut pre = lead.clone();
            if !carry.is_empty() {
                pre.push_str(&format!("{{{}}}", carry));
            }
            format!("{}{}", pre, p.trim_start())
        };
        // sonraki parçaya taşınacak açık biçimler (bu parçaya kadarki tüm metin)
        let upto: String = parts[..=k].concat();
        carry = open_styles(&upto);
        texts.push(body.trim_end().to_string());
    }
    let texts: Vec<String> = texts.into_iter().filter(|t| !tags::visible_text(t).trim().is_empty()).collect();
    if texts.len() < 2 {
        return vec![d.clone()];
    }
    let lens: Vec<usize> = texts.iter().map(|t| tags::visible_text(t).trim().chars().count().max(1)).collect();
    let total: usize = lens.iter().sum();
    let dur = d.duration().max(0);
    let mut t = d.start_time;
    let mut acc = 0usize;
    let n = texts.len();
    texts
        .into_iter()
        .enumerate()
        .map(|(k, text)| {
            let (s, e) = if keep_times {
                (d.start_time, d.end_time)
            } else {
                acc += lens[k];
                let e = if k + 1 == n { d.end_time } else { d.start_time + (dur as f64 * acc as f64 / total as f64).round() as i64 };
                let s = t;
                t = e;
                (s, e)
            };
            Dialogue { start_time: s, end_time: e, text, ..d.clone() }
        })
        .collect()
}

/// Karaoke satırını her hece bir olay olacak şekilde böler (Aegisub "Split by karaoke").
/// Baştaki karaoke dışı etiketler (konum, renk…) korunur; hece metninden `\k` atılır.
pub fn split_karaoke(d: &Dialogue) -> Vec<Dialogue> {
    let syl = tags::karaoke_syllables(&d.text);
    if syl.len() < 2 {
        return vec![d.clone()];
    }
    // baştaki bloktan karaoke etiketlerini çıkar
    let lead = match tags::split(&d.text).first() {
        Some(Part::Block(b)) => {
            let kept: Vec<&str> = b.split('\\').skip(1).filter(|t| !(t.starts_with('k') || t.starts_with('K')) || t.starts_with("kt")).collect();
            if kept.is_empty() {
                String::new()
            } else {
                format!("{{\\{}}}", kept.join("\\"))
            }
        }
        _ => String::new(),
    };
    let mut out = Vec::new();
    let mut t = d.start_time;
    for (cs, text) in syl {
        let dur = cs * 10;
        let s = t;
        t += dur;
        let vis = tags::strip_tags(&text);
        if vis.trim().is_empty() {
            continue;
        }
        out.push(Dialogue { start_time: s, end_time: t.min(d.end_time.max(s)), text: format!("{}{}", lead, vis.trim()), ..d.clone() });
    }
    if out.is_empty() {
        vec![d.clone()]
    } else {
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(t: &str) -> Dialogue {
        Dialogue { start_time: 1000, end_time: 4000, text: t.into(), ..Default::default() }
    }

    #[test]
    fn splits_at_breaks_proportionally() {
        let v = split_at_breaks(&d("{\\an8\\pos(10,10)}Birinci satır{\\i1}\\Nikinci{\\i0} uzun satır"), false);
        assert_eq!(v.len(), 2);
        assert_eq!(v[0].text, "{\\an8\\pos(10,10)}Birinci satır{\\i1}");
        assert_eq!(v[1].text, "{\\an8\\pos(10,10)}{\\i1}ikinci{\\i0} uzun satır");
        // 13 / 17 karakter → 3000 ms paylaşımı
        assert_eq!((v[0].start_time, v[0].end_time, v[1].start_time, v[1].end_time), (1000, 2300, 2300, 4000));
        let k = split_at_breaks(&d("a\\Nb\\N\\Nc"), true);
        assert_eq!(k.iter().map(|x| x.text.as_str()).collect::<Vec<_>>(), ["a", "b", "c"]);
        assert!(k.iter().all(|x| (x.start_time, x.end_time) == (1000, 4000)));
        assert_eq!(split_at_breaks(&d("tek satır"), false).len(), 1);
        // etiket içindeki \N bölünmez (çizim/clip koordinatı değil, metin)
        assert_eq!(split_at_breaks(&d("{\\clip(m 0 0 l 1 1)}a b"), false).len(), 1);
    }

    #[test]
    fn splits_karaoke_syllables() {
        let v = split_karaoke(&d("{\\an8\\k20}ka{\\k30}ra{\\k0}{\\k50}o ke"));
        assert_eq!(v.iter().map(|x| (x.start_time, x.end_time, x.text.as_str())).collect::<Vec<_>>(), vec![(1000, 1200, "{\\an8}ka"), (1200, 1500, "{\\an8}ra"), (1500, 2000, "{\\an8}o ke")]);
        assert_eq!(split_karaoke(&d("düz metin")).len(), 1);
    }
}
