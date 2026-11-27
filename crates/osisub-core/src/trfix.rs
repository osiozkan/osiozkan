//! Türkçe yaygın yazım hataları (Subtitle Edit'in dil başına düzeltme listeleri gibi).
//!
//! İki tür düzeltme, ikisi de yanlış alarm vermeyecek kadar dar tutulmuştur:
//! 1. Sabit liste: "herşey → her şey", "yada → ya da", "şuan → şu an", "malesef → maalesef"…
//!    (yalnızca tam sözcük; büyük harf korunur).
//! 2. Bitişik yazılmış soru eki, yalnızca kişi ekli biçimleri: "gelecekmisin → gelecek misin",
//!    "hazırmıyız → hazır mıyız". Kişi eksiz "-mi" (ör. "tamamı", "kalemi") ek de olabileceği için
//!    dokunulmaz.
use crate::fixes::map_text;

/// (yanlış, doğru) — küçük harfle; sözcük başı büyükse düzeltme de büyük başlar. Yalnızca gerçek
/// yazım hataları: bilinçli konuşma dili biçimleri ("napıyorsun", "gelcem") listede yok.
const WORDS: &[(&str, &str)] = &[
    ("herşey", "her şey"),
    ("herşeyi", "her şeyi"),
    ("herşeyin", "her şeyin"),
    ("herşeye", "her şeye"),
    ("herşeyden", "her şeyden"),
    ("birşey", "bir şey"),
    ("birşeyi", "bir şeyi"),
    ("birşeyler", "bir şeyler"),
    ("birşeyin", "bir şeyin"),
    ("birşeye", "bir şeye"),
    ("hiçbirşey", "hiçbir şey"),
    ("hiçbirşeyi", "hiçbir şeyi"),
    ("hiç birşey", "hiçbir şey"),
    ("yada", "ya da"),
    ("şuan", "şu an"),
    ("şuanda", "şu anda"),
    ("malesef", "maalesef"),
    ("herkez", "herkes"),
    ("herkeze", "herkese"),
    ("yanlız", "yalnız"),
    ("yalnış", "yanlış"),
    ("orjinal", "orijinal"),
    ("birsürü", "bir sürü"),
    ("birkere", "bir kere"),
    ("herzaman", "her zaman"),
    ("hergün", "her gün"),
    ("heryer", "her yer"),
    ("heryerde", "her yerde"),
    ("hiçkimse", "hiç kimse"),
    ("pekçok", "pek çok"),
    ("yalnızmı", "yalnız mı"),
    ("eşşek", "eşek"),
    ("kirbit", "kibrit"),
    ("traş", "tıraş"),
    ("ünvan", "unvan"),
    ("deyil", "değil"),
    ("diil", "değil"),
    ("diyil", "değil"),
    ("allahım", "Allah'ım"),
];

/// Kişi ekli soru eki (bitişik yazıldıysa ayrılır).
const PARTICLES: &[&str] = &[
    "misiniz", "mısınız", "musunuz", "müsünüz", "miyiz", "mıyız", "muyuz", "müyüz", "misin", "mısın", "musun", "müsün", "miyim", "mıyım", "muyum", "müyüm", "miydi", "mıydı", "muydu", "müydü", "miymiş", "mıymış", "muymuş", "müymüş",
];

fn lower(s: &str) -> String {
    crate::qc::tr_lower(s)
}

fn cap_like(orig: &str, repl: &str) -> String {
    let first_upper = orig.chars().next().map_or(false, |c| c.is_uppercase());
    let all_upper = orig.chars().filter(|c| c.is_alphabetic()).count() > 1 && orig.chars().filter(|c| c.is_alphabetic()).all(|c| c.is_uppercase());
    if all_upper {
        crate::qc::tr_upper(repl)
    } else if first_upper {
        let mut it = repl.chars();
        match it.next() {
            Some(c) => crate::qc::tr_upper(&c.to_string()) + it.as_str(),
            None => String::new(),
        }
    } else {
        repl.to_string()
    }
}

/// Soru eki ayrılabilecek kök mü? "adamıyım" (adam-ı-yım), "hanımıyım", "kalemiydi", "resmiymiş"
/// gibi iyelik + ek-fiil biçimleri de "-mıyım/-miydi" ile biter; kökü sözlüksüz ayırt etmek için
/// yalnızca fiil çekimi (-ecek/-acak, -iyor, -miş, geniş zaman -ır/-er…) ya da sık sıfat
/// köklerinde ayrılır. Emin olunamayan biçime dokunulmaz.
fn verbal_root(root: &str) -> bool {
    const ENDS: &[&str] = &[
        "ecek", "acak", "iyor", "ıyor", "uyor", "üyor", "yor", "miş", "mış", "muş", "müş", "meli", "malı", "abilir", "ebilir",
        "ır", "ir", "ur", "ür", "ar", "er", "dı", "di", "du", "dü", "tı", "ti", "tu", "tü",
    ];
    const ADJ: &[&str] = &[
        "iyi", "emin", "hazır", "mutlu", "deli", "ciddi", "hasta", "yalnız", "aç", "tok", "uygun", "rahat", "memnun", "sağ", "kötü", "güzel",
        "doğru", "yanlış", "burada", "orada", "şurada", "evde", "gerçek", "kızgın", "üzgün", "korkak", "aptal", "sarhoş", "yorgun", "meşgul",
    ];
    // geniş zaman ekleri kısa: kök en az 4 harf olsun ("ser-mi" gibi kısa kökler sözcüğün parçası olabilir)
    ADJ.contains(&root) || ENDS.iter().any(|e| root.ends_with(e) && root.chars().count() >= e.chars().count() + 2)
}

/// Tek sözcüğün düzeltmesi (noktalama hariç), yoksa None.
fn fix_word(w: &str) -> Option<String> {
    let l = lower(w);
    if let Some((_, good)) = WORDS.iter().find(|(bad, good)| *bad == l && bad != good) {
        return Some(cap_like(w, good));
    }
    for p in PARTICLES {
        if let Some(root) = l.strip_suffix(p) {
            // kök en az 3 harf; "kimsiniz" gibi kökler yine de 3+ olur ama "mi" ile bitmez → dokunulmaz
            if root.chars().count() >= 3 && root.chars().all(|c| c.is_alphabetic()) && verbal_root(root) {
                let n = root.chars().count();
                let orig_root: String = w.chars().take(n).collect();
                let orig_p: String = w.chars().skip(n).collect();
                return Some(format!("{} {}", orig_root, orig_p));
            }
        }
    }
    None
}

/// Etiketler dışındaki metinde düzeltmeleri uygular; (yeni metin, düzeltme sayısı).
pub fn fix_text(text: &str) -> (String, usize) {
    let mut n = 0;
    let out = map_text(text, |t| {
        // önce iki sözcüklü kalıplar ("hiç birşey")
        let mut s = t.to_string();
        for (bad, good) in WORDS.iter().filter(|(b, g)| b.contains(' ') && b != g) {
            let l = lower(&s);
            if let Some(i) = l.find(bad) {
                let before_ok = i == 0 || !l[..i].chars().last().map_or(false, |c| c.is_alphanumeric());
                let after = i + bad.len();
                let after_ok = after >= l.len() || !l[after..].chars().next().map_or(false, |c| c.is_alphanumeric());
                // tr_lower bayt uzunluğunu değiştirmez mi? (İ → i̇ değişebilir) — yalnızca uzunluklar eşitse uygula
                if before_ok && after_ok && l.len() == s.len() {
                    s = format!("{}{}{}", &s[..i], cap_like(&s[i..after], good), &s[after..]);
                    n += 1;
                }
            }
        }
        // tek sözcükler
        let mut out = String::with_capacity(s.len());
        let mut word = String::new();
        let flush = |word: &mut String, out: &mut String, n: &mut usize| {
            if !word.is_empty() {
                match fix_word(word) {
                    Some(f) => {
                        out.push_str(&f);
                        *n += 1;
                    }
                    None => out.push_str(word),
                }
                word.clear();
            }
        };
        for c in s.chars() {
            if c.is_alphabetic() || c == '\'' && !word.is_empty() {
                word.push(c);
            } else {
                flush(&mut word, &mut out, &mut n);
                out.push(c);
            }
        }
        flush(&mut word, &mut out, &mut n);
        out
    });
    (out, n)
}

/// QC için: metinde düzeltilecek sözcükler (yanlış → doğru).
pub fn findings(text: &str) -> Vec<(String, String)> {
    let mut v = Vec::new();
    for w in crate::tags::visible_text(text).split(|c: char| !(c.is_alphabetic() || c == '\'')) {
        if w.is_empty() {
            continue;
        }
        if let Some(f) = fix_word(w) {
            v.push((w.to_string(), f));
        }
    }
    v
}

/// Bilerek yinelenen sözcükler (Türkçe ikilemeler, ünlemler, İngilizce "had had").
const DOUBLE_OK: &[&str] = &[
    "yavaş", "tek", "bir", "iki", "üç", "ara", "sık", "kat", "ufak", "parça", "sıra", "yan", "çabuk", "hızlı", "derin", "kapı", "ev", "yıl",
    "gün", "saat", "adım", "ayrı", "damla", "bölük", "grup", "kucak", "avuç", "sıcak", "soğuk", "yeni", "güzel", "iyi", "renk", "çeşit", "türlü",
    "yer", "köşe", "dal", "taze", "ince", "kalın", "uzun", "kısa", "büyük", "küçük", "öbek", "deste", "tane", "kilo", "metre", "saniye", "dakika",
    "hayır", "evet", "hey", "oh", "ah", "aa", "ha", "he", "hı", "hm", "tamam", "dur", "bekle", "hadi", "haydi", "gel", "git", "koş", "yok", "çok",
    "no", "yes", "oh", "wait", "come", "go", "hey", "okay", "ok", "very", "so", "had", "that", "bye", "now", "run", "stop", "please", "sorry",
    "la", "na", "da", "ya", "ra", "ta", "lala", "dum", "boom", "bang", "knock", "tık", "pat", "çat", "güm", "hop",
];

/// Arka arkaya yinelenen sözcükler (Subtitle Edit "Find double words"): yalnız boşlukla ayrılmış aynı
/// sözcük (harf duyarsız, Türkçe i/ı). İkilemeler (`yavaş yavaş`), -A ulaçları (`koşa koşa`, `güle güle`)
/// ve ünlemler (`hayır hayır`) atlanır; noktalama araya girerse (`Evet, evet`) yineleme sayılmaz.
pub fn double_words(text: &str) -> Vec<String> {
    let vis = crate::tags::visible_text(text);
    let mut out = vec![];
    for line in vis.lines() {
        let toks: Vec<&str> = line.split(' ').filter(|t| !t.is_empty()).collect();
        for w in toks.windows(2) {
            let (a, b) = (w[0], w[1]);
            // ilk sözcükte sondaki noktalama yinelemeyi böler
            if !a.chars().last().is_some_and(|c| c.is_alphanumeric()) { continue; }
            let core_b = b.trim_end_matches(|c: char| !c.is_alphanumeric());
            if !a.chars().all(|c| c.is_alphabetic() || c == '\'') || core_b.is_empty() { continue; }
            let (la, lb) = (crate::qc::tr_lower(a), crate::qc::tr_lower(core_b));
            if la != lb { continue; }
            let n = la.chars().count();
            let gerund = n >= 4 && (la.ends_with('a') || la.ends_with('e'));
            if DOUBLE_OK.contains(&la.as_str()) || gerund { continue; }
            out.push(format!("{a} {core_b}"));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fixes_common_turkish_misspellings() {
        assert_eq!(fix_text("Herşey yolunda, yada değil.").0, "Her şey yolunda, ya da değil.");
        assert_eq!(fix_text("{\\i1}Şuan gelecekmisin?{\\i0}").0, "{\\i1}Şu an gelecek misin?{\\i0}");
        assert_eq!(fix_text("HERŞEY TAMAM").0, "HER ŞEY TAMAM");
        assert_eq!(fix_text("Hazırmıyız? Malesef hiç birşey yok.").0, "Hazır mıyız? Maalesef hiçbir şey yok.");
        // dokunulmayanlar: ek olabilecek -mi, doğru yazımlar, etiket içi
        for ok in ["Tamamı bitti mi?", "Kalemi nerede?", "Kimsin sen?", "Bir şey yok.", "{\\fnHerşey}a", "Gelecek misin?", "Admiral"] {
            assert_eq!(fix_text(ok).0, ok, "{ok}");
        }
        assert_eq!(fix_text("herşey, birşey").1, 2);
        assert_eq!(findings("Şuan gelecekmisin?"), vec![("Şuan".to_string(), "Şu an".to_string()), ("gelecekmisin".to_string(), "gelecek misin".to_string())]);
    }

    #[test]
    fn inceleme_yanlis_ayirma_yok() {
        for w in ["adamıyım", "adamısın", "adamıydı", "hanımıyım", "kalemiydi", "resmiymiş", "tamamıydı", "sistemiydi", "Kimi", "Emi", "Hepimiz"] {
            assert_eq!(fix_text(w).0, w, "{w} bozulmamalı");
        }
        for (w, want) in [("gelecekmisin", "gelecek misin"), ("hazırmıyız", "hazır mıyız"), ("geliyormusun", "geliyor musun"), ("iyimisin", "iyi misin"), ("yapabilirmiyim", "yapabilir miyim"), ("gelmişmiydi", "gelmiş miydi")] {
            assert_eq!(fix_text(w).0, want);
        }
    }

    #[test]
    fn double_words_found() {
        assert_eq!(double_words("I saw the the cat"), vec!["the the"]);
        assert_eq!(double_words("{\\i1}Bunu bunu{\\i0} yapma\\Nve ve git"), vec!["Bunu bunu", "ve ve"]);
        assert_eq!(double_words("Kim kim? Sen sen!"), vec!["Kim kim", "Sen sen"]);
        // ikilemeler, ulaçlar, ünlemler, noktalama
        assert!(double_words("Yavaş yavaş, koşa koşa, güle güle! Hayır hayır. Evet, evet. Bir bir saydı.").is_empty());
        assert!(double_words("A-a-ama 3 3").is_empty());
        // satır sonu yinelemeyi bölmez ama satırlar arası sayılmaz
        assert!(double_words("sen\\Nsen").is_empty());
    }
}
