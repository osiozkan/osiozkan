//! Düz metin içe aktarma (Subtitle Edit "Import plain text"): çevirmenin metin belgesindeki
//! satırları altyazıya çevirir — yeni zamanlanmış satırlar olarak ya da var olan zamanlamaya
//! sırayla yerleştirerek (fansub: zamanlayıcı ve çevirmen ayrı çalışır).
use crate::ass::{Dialogue, Entry, Script};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Split {
    /// Her boş olmayan satır bir altyazı
    Lines,
    /// Boş satırla ayrılmış bloklar; blok içindeki satırlar `\N` ile birleşir
    Blocks,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Item {
    pub actor: String,
    pub text: String,
}

/// "Ad: replik" öneki: ad kısa (≤ 24 karakter, en çok 3 sözcük), harfle başlar, iki nokta sonrası
/// boşluk gelir; saat (12:30) ya da URL sayılmaz.
fn split_speaker(line: &str) -> Option<(String, String)> {
    let (name, rest) = line.split_once(':')?;
    let name = name.trim();
    if name.is_empty() || name.chars().count() > 24 || name.split_whitespace().count() > 3 || !rest.starts_with(' ') {
        return None;
    }
    // ad: harfle başlar; harf, rakam, boşluk, nokta, tire, kesme (Guard 2, Dr. Who); URL değil
    if !name.chars().next()?.is_alphabetic() || name.contains("//") || !name.chars().all(|c| c.is_alphanumeric() || " .-'’".contains(c)) {
        return None;
    }
    Some((name.to_string(), rest.trim().to_string()))
}

/// Metni öğelere böler. `speakers`: "Ad: replik" önekini konuşana ayır. `#` ya da `//` ile
/// başlayan satırlar yorum sayılıp atlanır.
pub fn parse(text: &str, split: Split, speakers: bool) -> Vec<Item> {
    let text = text.replace("\r\n", "\n").replace('\r', "\n");
    let mut out = vec![];
    let push = |lines: &[&str], out: &mut Vec<Item>| {
        let mut actor = String::new();
        let mut parts: Vec<String> = vec![];
        for (k, l) in lines.iter().enumerate() {
            let l = l.trim();
            // dışa aktarımın kaçışları: "\#…" / "\//…" (yorum sanılmasın), ": metin" (ad yok)
            let l = l.strip_prefix('\\').filter(|r| r.starts_with('#') || r.starts_with("//")).unwrap_or(l);
            if speakers && k == 0 {
                if let Some(r) = l.strip_prefix(": ") { parts.push(r.to_string()); continue; }
            }
            match (speakers, k == 0, split_speaker(l)) {
                (true, true, Some((a, r))) => { actor = a; parts.push(r); }
                _ => parts.push(l.to_string()),
            }
        }
        let t = parts.into_iter().filter(|p| !p.is_empty()).collect::<Vec<_>>().join("\\N");
        if !t.is_empty() {
            out.push(Item { actor, text: t });
        }
    };
    let keep = |l: &&str| { let t = l.trim(); !(t.starts_with('#') || t.starts_with("//")) };
    match split {
        Split::Lines => {
            for l in text.lines().filter(keep).filter(|l| !l.trim().is_empty()) {
                push(&[l], &mut out);
            }
        }
        Split::Blocks => {
            let mut block: Vec<&str> = vec![];
            for l in text.lines().filter(keep) {
                if l.trim().is_empty() {
                    if !block.is_empty() { push(&block, &mut out); block.clear(); }
                } else {
                    block.push(l);
                }
            }
            if !block.is_empty() { push(&block, &mut out); }
        }
    }
    out
}

/// Yeni satırlar: `start`tan itibaren, süre okuma hızından (`cps`, en az `min_ms`, en çok
/// `max_ms`), satırlar arası `gap_ms`.
pub fn to_dialogues(items: &[Item], start: i64, cps: f64, min_ms: i64, max_ms: i64, gap_ms: i64, style: &str) -> Vec<Dialogue> {
    let mut t = start;
    items.iter().map(|it| {
        let chars = crate::tags::visible_text(&it.text).chars().filter(|c| !c.is_whitespace()).count() as f64;
        let dur = ((chars / cps.max(1.0)) * 1000.0).round() as i64;
        let dur = dur.clamp(min_ms, max_ms.max(min_ms));
        let d = Dialogue { start_time: t, end_time: t + dur, style: style.to_string(), actor: it.actor.clone(), text: it.text.clone(), ..Default::default() };
        t += dur + gap_ms;
        d
    }).collect()
}

/// Var olan zamanlamaya yerleştir: `from` girdisinden başlayarak sıradaki diyalog satırlarının
/// (yorumlar ve tipografi/karaoke satırları atlanır) metni öğelerle değiştirilir; `actors` ise
/// konuşan da yazılır (öğede ad varsa). Dönüş: (yerleştirilen, artan öğe, değişen girdi indeksleri).
pub fn fill_existing(script: &mut Script, items: &[Item], from: usize, actors: bool) -> (usize, usize, Vec<usize>) {
    let mut it = items.iter();
    let mut n = 0;
    let mut touched = vec![];
    for (i, e) in script.entries.iter_mut().enumerate().skip(from) {
        let Entry::Dialogue(d) = e else { continue };
        if d.comment || crate::qc::is_typeset(d) || crate::qc::is_karaoke(d) || d.effect.eq_ignore_ascii_case("fx") {
            continue;
        }
        let Some(x) = it.next() else { break };
        // baştaki etiket blokları (ör. {\an8}{\i1}) korunur; karaoke blokları hariç
        let lead: String = crate::tags::split(crate::tags::leading_blocks(&d.text)).into_iter().filter_map(|p| match p {
            crate::tags::Part::Block(b) if !b.contains("\\k") => Some(format!("{{{b}}}")),
            _ => None,
        }).collect();
        // dışa aktarımda boş satır "-", gerçek "-" metni "\-" olarak yazılır
        d.text = match x.text.as_str() { "-" => lead, "\\-" => format!("{lead}-"), t => format!("{lead}{t}") };
        if actors && !x.actor.is_empty() {
            d.actor = x.actor.clone();
        }
        n += 1;
        touched.push(i);
    }
    (n, it.count(), touched)
}

/// Çevirmene gönderilecek düz metin: `fill_existing`'in atladığı satırlar (yorum, tabela, karaoke,
/// fx) hariç, her diyalog bir satır (`\N` → " / " yerine boşluk, etiketler çıkarılır); `speakers`
/// ise "Ad: " öneki. İçe aktarıldığında aynı satırlara sırayla geri yerleşir.
pub fn export(script: &Script, speakers: bool) -> String {
    let mut out = String::new();
    for d in script.dialogues() {
        if d.comment || crate::qc::is_typeset(d) || crate::qc::is_karaoke(d) || d.effect.eq_ignore_ascii_case("fx") {
            continue;
        }
        let t = crate::tags::visible_text(&d.text).replace('\n', " ").split_whitespace().collect::<Vec<_>>().join(" ");
        let t = if t.is_empty() { "-".to_string() } else { t };
        if speakers && !d.actor.trim().is_empty() && split_speaker(&format!("{}: x", d.actor.trim())).is_some() {
            out.push_str(&format!("{}: {}\n", d.actor.trim(), t));
            continue;
        }
        // satır başında yorum işareti varsa kaçış (ad önekli satırda gerekmez)
        let t = if t.starts_with('#') || t.starts_with("//") { format!("\\{t}") } else { t };
        // metni tam olarak "-" olan satır boş satır işaretiyle karışmasın
        let t = if t == "-" && crate::tags::visible_text(&d.text).trim() == "-" { "\\-".to_string() } else { t };
        if speakers && (split_speaker(&t).is_some() || t.starts_with(": ")) {
            // ad gibi görünen metin ("Dinle: geliyorlar") geri alınırken ada dönüşmesin
            out.push_str(&format!(": {t}\n"));
        } else {
            out.push_str(&t);
            out.push('\n');
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ayristir() {
        let t = "Hinata: Merhaba!\r\n\r\n# not\nKageyama: Geç kaldın.\nSaat 12:30 oldu bile.\n";
        let v = parse(t, Split::Lines, true);
        assert_eq!(v, vec![
            Item { actor: "Hinata".into(), text: "Merhaba!".into() },
            Item { actor: "Kageyama".into(), text: "Geç kaldın.".into() },
            Item { actor: "".into(), text: "Saat 12:30 oldu bile.".into() },
        ]);
        let b = parse("Bir\niki\n\n\nÜç\n", Split::Blocks, false);
        assert_eq!(b.iter().map(|i| i.text.as_str()).collect::<Vec<_>>(), ["Bir\\Niki", "Üç"]);
        // ad önekini kapatınca metin aynen
        assert_eq!(parse("Ad: x", Split::Lines, false)[0].text, "Ad: x");
        // URL / uzun önek ad değildir
        assert_eq!(parse("http://a.b: x", Split::Lines, true)[0].actor, "");
        assert_eq!(parse("Bu çok uzun bir cümlenin başı: devamı", Split::Lines, true)[0].actor, "");
    }

    #[test]
    fn tire_ve_coklu_bas_blok_gidis_donus() {
        let mut sc = Script::from_dialogues(vec![
            Dialogue { text: "-".into(), ..Default::default() },
            Dialogue { text: "".into(), ..Default::default() },
            Dialogue { text: "{\\an8}{\\i1}üst".into(), ..Default::default() },
        ]);
        let txt = export(&sc, false);
        let items = parse(&txt, Split::Lines, false);
        for d in sc.dialogues_mut() { d.text = osisub_core_dummy(&d.text); }
        fill_existing(&mut sc, &items, 0, false);
        let t: Vec<String> = sc.dialogues().map(|d| d.text.clone()).collect();
        assert_eq!(t, vec!["-", "", "{\\an8}{\\i1}üst"]);
    }

    /// içe aktarmadan önce metni boz (yalnız baş blokları kalsın) — yerleştirmenin gerçekten yazdığını görmek için
    fn osisub_core_dummy(t: &str) -> String {
        format!("{}ESKİ", crate::tags::leading_blocks(t))
    }

    #[test]
    fn yeni_satirlar() {
        let v = parse("Kısa.\nBu satır biraz daha uzun bir cümledir.", Split::Lines, false);
        let d = to_dialogues(&v, 1000, 15.0, 1000, 7000, 100, "Default");
        assert_eq!((d[0].start_time, d[0].end_time), (1000, 2000)); // min süre
        assert_eq!(d[1].start_time, 2100);
        assert!(d[1].end_time - d[1].start_time > 2000);
    }

    #[test]
    fn disa_aktar_ve_geri_yerlestir() {
        let mut s = Script::new_default();
        s.entries.retain(|e| !matches!(e, Entry::Dialogue(_)));
        for (a, t) in [("Hinata", "{\\i1}Merhaba\\Ndünya"), ("", "{\\pos(1,2)}TABELA"), ("", "Adsız satır")] {
            s.entries.push(Entry::Dialogue(Dialogue { actor: a.into(), text: t.into(), ..Default::default() }));
        }
        let txt = export(&s, true);
        assert_eq!(txt, "Hinata: Merhaba dünya\nAdsız satır\n");
        let items = parse(&txt.replace("Merhaba dünya", "Selam dünya"), Split::Lines, true);
        let first = s.entries.iter().position(|e| matches!(e, Entry::Dialogue(_))).unwrap();
        assert_eq!(fill_existing(&mut s, &items, first, true).0, 2);
        assert_eq!(s.dialogues().next().unwrap().text, "{\\i1}Selam dünya");
    }

    #[test]
    fn gidis_donus_kacislari() {
        let mut s = Script::new_default();
        s.entries.retain(|e| !matches!(e, Entry::Dialogue(_)));
        for (a, t) in [("", "#1 hayranın benim"), ("", "Dinle: geliyorlar"), ("Guard 2", "Dur!"), ("", ""), ("Dr. Who", "// yorum değil")] {
            s.entries.push(Entry::Dialogue(Dialogue { actor: a.into(), text: t.into(), ..Default::default() }));
        }
        let before: Vec<(String, String)> = s.dialogues().map(|d| (d.actor.clone(), d.text.clone())).collect();
        let txt = export(&s, true);
        let items = parse(&txt, Split::Lines, true);
        assert_eq!(items.len(), 5, "{txt}");
        let first = s.entries.iter().position(|e| matches!(e, Entry::Dialogue(_))).unwrap();
        for e in s.entries.iter_mut() { if let Entry::Dialogue(d) = e { d.text = "X".into(); d.actor.clear(); } }
        fill_existing(&mut s, &items, first, true);
        let after: Vec<(String, String)> = s.dialogues().map(|d| (d.actor.clone(), d.text.clone())).collect();
        assert_eq!(after, before, "{txt}");
    }

    #[test]
    fn mevcut_zamanlamaya() {
        let mut s = Script::new_default();
        s.entries.retain(|e| !matches!(e, Entry::Dialogue(_)));
        for (t, c, e) in [("orijinal 1", false, ""), ("{\\an8}orijinal 2", false, ""), ("not", true, ""), ("{\\pos(1,2)}TABELA", false, ""), ("orijinal 3", false, "")] {
            s.entries.push(Entry::Dialogue(Dialogue { text: t.into(), comment: c, effect: e.into(), ..Default::default() }));
        }
        let first = s.entries.iter().position(|e| matches!(e, Entry::Dialogue(_))).unwrap();
        let items = parse("A: bir\niki\nüç\ndört", Split::Lines, true);
        let (n, left, touched) = fill_existing(&mut s, &items, first, true);
        assert_eq!((n, left, touched.len()), (3, 1, 3));
        let ds: Vec<(String, String)> = s.dialogues().map(|d| (d.actor.clone(), d.text.clone())).collect();
        assert_eq!(ds[0], ("A".into(), "bir".into()));
        assert_eq!(ds[1].1, "{\\an8}iki");
        assert_eq!(ds[2].1, "not");
        assert_eq!(ds[3].1, "{\\pos(1,2)}TABELA");
        assert_eq!(ds[4].1, "üç");
    }
}
