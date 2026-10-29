//! osisub-spell: Hunspell uyumlu yazım denetimi (kullanıcının sağladığı .aff/.dic ile; ör. tr_TR).
use osisub_core::ass::{Entry, Script};
use osisub_core::tags;
use std::path::{Path, PathBuf};

const FLAG_DIRECTIVES: &[&str] = &["SFX", "PFX", "KEEPCASE", "NEEDAFFIX", "FORBIDDENWORD", "ONLYINCOMPOUND", "COMPOUNDFLAG", "NOSUGGEST", "WARN", "CIRCUMFIX", "COMPOUNDBEGIN", "COMPOUNDMIDDLE", "COMPOUNDEND", "COMPOUNDPERMITFLAG", "COMPOUNDFORBIDFLAG", "LEMMA_PRESENT", "SUBSTANDARD"];

/// `FLAG num` sözlüklerinde tüm sayısal bayrakları +1 kaydırır (0 → 1).
fn shift_aff_flags(aff: &str) -> String {
    let mut out = String::with_capacity(aff.len() + aff.len() / 16);
    for line in aff.lines() {
        let mut it = line.splitn(3, char::is_whitespace);
        let (d, f, rest) = (it.next().unwrap_or(""), it.next().unwrap_or(""), it.next());
        if FLAG_DIRECTIVES.contains(&d) {
            if let Ok(n) = f.parse::<u32>() {
                out.push_str(d);
                out.push(' ');
                out.push_str(&(n + 1).to_string());
                if let Some(r) = rest {
                    out.push(' ');
                    out.push_str(r);
                }
                out.push('\n');
                continue;
            }
        }
        out.push_str(line);
        out.push('\n');
    }
    out
}

fn shift_dic_flags(dic: &str) -> String {
    let mut out = String::with_capacity(dic.len() + dic.len() / 16);
    for (i, line) in dic.lines().enumerate() {
        if i == 0 {
            out.push_str(line);
            out.push('\n');
            continue;
        }
        match line.find('/') {
            Some(p) => {
                out.push_str(&line[..=p]);
                let rest = &line[p + 1..];
                let end = rest.find(char::is_whitespace).unwrap_or(rest.len());
                let flags: Vec<String> = rest[..end].split(',').map(|f| f.parse::<u32>().map(|n| (n + 1).to_string()).unwrap_or_else(|_| f.to_string())).collect();
                out.push_str(&flags.join(","));
                out.push_str(&rest[end..]);
            }
            None => out.push_str(line),
        }
        out.push('\n');
    }
    out
}

pub struct Speller {
    dict: spellbook::Dictionary,
    /// Kullanıcının eklediği sözcükler (küçük harf, Türkçe kurallı).
    extra: Vec<String>,
    pub name: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Miss {
    /// `Script::entries` indeksi.
    pub entry: usize,
    /// Satır metnindeki (etiketsiz görünür metin) bayt aralığı değil, sözcüğün kendisi.
    pub word: String,
}

/// Olası sözlük konumlarında `<ad>.aff` + `<ad>.dic` arar (ör. `tr_TR`).
pub fn find_dictionary(name: &str, extra_dirs: &[PathBuf]) -> Option<(PathBuf, PathBuf)> {
    let mut dirs: Vec<PathBuf> = extra_dirs.to_vec();
    dirs.extend(["/usr/share/hunspell", "/usr/share/myspell", "/usr/share/myspell/dicts", "/Library/Spelling", "C:\\Program Files\\LibreOffice\\share\\extensions"].iter().map(PathBuf::from));
    if let Ok(h) = std::env::var("HOME") {
        dirs.push(PathBuf::from(&h).join(".config/osisub/dict"));
        dirs.push(PathBuf::from(&h).join(".config/libreoffice/4/user/wordbook"));
    }
    for d in dirs {
        let (aff, dic) = (d.join(format!("{}.aff", name)), d.join(format!("{}.dic", name)));
        if aff.is_file() && dic.is_file() {
            return Some((aff, dic));
        }
    }
    None
}

impl Speller {
    pub fn load(aff: &Path, dic: &Path) -> Result<Speller, String> {
        let a = std::fs::read_to_string(aff).map_err(|e| format!("{}: {}", aff.display(), e))?;
        let d = std::fs::read_to_string(dic).map_err(|e| format!("{}: {}", dic.display(), e))?;
        let dict = match spellbook::Dictionary::new(&a, &d) {
            Ok(dict) => dict,
            // Bazı sözlükler (ör. LibreOffice tr_TR) "FLAG num" ile 0 numaralı bayrak kullanır; spellbook bunu kabul etmez.
            Err(e) if e.to_string().contains("flag cannot be zero") => {
                let (a2, d2) = (shift_aff_flags(&a), shift_dic_flags(&d));
                spellbook::Dictionary::new(&a2, &d2).map_err(|e| format!("sözlük çözümlenemedi: {}", e))?
            }
            Err(e) => return Err(format!("sözlük çözümlenemedi: {}", e)),
        };
        Ok(Speller { dict, extra: vec![], name: aff.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default() })
    }

    pub fn add_word(&mut self, w: &str) {
        self.extra.push(osisub_core::qc::tr_lower(w));
    }

    pub fn check(&self, word: &str) -> bool {
        if word.is_empty() || word.chars().all(|c| !c.is_alphabetic()) {
            return true;
        }
        if self.extra.contains(&osisub_core::qc::tr_lower(word)) {
            return true;
        }
        self.dict.check(word) || self.dict.check(&osisub_core::qc::tr_lower(word))
    }

    pub fn suggest(&self, word: &str, max: usize) -> Vec<String> {
        let mut out = Vec::new();
        self.dict.suggest(word, &mut out);
        out.truncate(max);
        out
    }
}

/// Metindeki denetlenecek sözcükler (etiketler ve \N atlanır; kesme işaretiyle ayrılan ekler çıkarılır: "İstanbul'da" → "İstanbul").
pub fn words_of(text: &str) -> Vec<String> {
    let vis = tags::visible_text(text);
    let mut words = vec![];
    let mut cur = String::new();
    let flush = |cur: &mut String, words: &mut Vec<String>| {
        let w = cur.trim_matches(|c: char| !c.is_alphanumeric()).to_string();
        if !w.is_empty() {
            words.push(w);
        }
        cur.clear();
    };
    let mut skip_suffix = false;
    for c in vis.chars() {
        if c.is_alphanumeric() {
            if !skip_suffix {
                cur.push(c);
            }
        } else if c == '\'' || c == '’' {
            // Türkçe ek ayırma işareti: sonrası ek, denetleme.
            if !cur.is_empty() {
                skip_suffix = true;
            }
        } else if c == '-' && !cur.is_empty() {
            cur.push(c);
        } else {
            skip_suffix = false;
            flush(&mut cur, &mut words);
        }
    }
    flush(&mut cur, &mut words);
    words
}

/// Metindeki `word` sözcüğünü (tam sözcük, etiketler dışı) `repl` ile değiştirir; `all` değilse yalnızca ilkini.
pub fn replace_word(text: &str, word: &str, repl: &str, all: bool) -> String {
    let mut done = false;
    osisub_core::fixes::map_text(text, |t| {
        if done && !all {
            return t.to_string();
        }
        let chars: Vec<char> = t.chars().collect();
        let w: Vec<char> = word.chars().collect();
        let mut out = String::new();
        let mut i = 0;
        while i < chars.len() {
            let is_match = !w.is_empty()
                && (all || !done)
                && chars[i..].starts_with(&w)
                && (i == 0 || !chars[i - 1].is_alphanumeric())
                && chars.get(i + w.len()).map_or(true, |c| !c.is_alphanumeric() || *c == '\'' || *c == '’');
            if is_match {
                out.push_str(repl);
                i += w.len();
                done = true;
            } else {
                out.push(chars[i]);
                i += 1;
            }
        }
        out
    })
}

/// Tüm (yorum olmayan) satırlardaki yanlış yazılmış sözcükler.
pub fn check_script(sp: &Speller, script: &Script, only: Option<&[usize]>) -> Vec<Miss> {
    let mut out = vec![];
    for (i, e) in script.entries.iter().enumerate() {
        let Entry::Dialogue(d) = e else { continue };
        if d.comment || only.map_or(false, |o| !o.contains(&i)) {
            continue;
        }
        for w in words_of(&d.text) {
            // Tamamen büyük harfli kısa sözcükler (kısaltma), rakam içerenler atlanır.
            // Latin olmayan yazılar (Japonca, Kiril ...) bu sözlükle denetlenemez.
            let latin = |c: char| (c as u32) < 0x0250 || (0x1E00..=0x1EFF).contains(&(c as u32));
            if w.chars().any(|c| c.is_alphabetic() && !latin(c)) {
                continue;
            }
            if w.chars().any(|c| c.is_numeric()) || (w.chars().count() <= 4 && w.chars().all(|c| c.is_uppercase())) {
                continue;
            }
            if !sp.check(&w) {
                out.push(Miss { entry: i, word: w });
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const AFF: &str = "SET UTF-8\nTRY esianrtolcdugmphbyfvkwzESIANRTOLCDUGMPHBYFVKWZ'\nSFX S Y 1\nSFX S 0 s .\n";
    const DIC: &str = "4\nhello/S\nworld\nİstanbul\nmerhaba\n";

    fn sp() -> Speller {
        Speller { dict: spellbook::Dictionary::new(AFF, DIC).unwrap(), extra: vec![], name: "test".into() }
    }

    #[test]
    fn check_and_suggest() {
        let s = sp();
        assert!(s.check("hello") && s.check("hellos") && s.check("world"));
        assert!(!s.check("wrold"));
        assert!(s.suggest("wrold", 5).contains(&"world".to_string()));
        assert!(s.check("123") && s.check("--"));
    }

    #[test]
    fn words_and_apostrophe() {
        assert_eq!(words_of("{\\i1}İstanbul'da{\\i0} merhaba\\Ndünya!"), vec!["İstanbul", "merhaba", "dünya"]);
        assert_eq!(words_of("e-posta (test)"), vec!["e-posta", "test"]);
    }

    #[test]
    fn skips_non_latin() {
        let s = sp();
        let script = Script::from_dialogues(vec![osisub_core::Dialogue { text: "学校の屋上 wrold".into(), ..Default::default() }]);
        assert_eq!(check_script(&s, &script, None).len(), 1);
    }

    #[test]
    fn replace_word_whole_only() {
        assert_eq!(replace_word("wrold {\\i1}wrold{\\i0} wroldx", "wrold", "world", false), "world {\\i1}wrold{\\i0} wroldx");
        assert_eq!(replace_word("wrold wrold'da wroldx", "wrold", "world", true), "world world'da wroldx");
    }

    #[test]
    fn script_check_skips_acronyms() {
        let mut s = sp();
        let script = Script::from_dialogues(vec![osisub_core::Dialogue { text: "hello wrold NASA x2 merhaba".into(), ..Default::default() }]);
        let m = check_script(&s, &script, None);
        assert_eq!(m.iter().map(|x| x.word.as_str()).collect::<Vec<_>>(), vec!["wrold"]);
        s.add_word("wrold");
        assert!(check_script(&s, &script, None).is_empty());
    }
}

#[cfg(test)]
mod real_dict {
    use super::*;

    /// Gerçek tr_TR sözlüğü varsa (OSISUB_TR_DICT=<klasör>) çalışır.
    #[test]
    fn turkish_dictionary_if_available() {
        let Ok(dir) = std::env::var("OSISUB_TR_DICT") else { return };
        let (aff, dic) = find_dictionary("tr_TR", &[PathBuf::from(dir)]).expect("sözlük bulunamadı");
        let t = std::time::Instant::now();
        let s = Speller::load(&aff, &dic).unwrap();
        eprintln!("sözlük yükleme: {:?}", t.elapsed());
        for ok in ["merhaba", "günaydın", "Bugün", "güzel", "ödevi", "unutma", "ışık", "İstanbul"] {
            assert!(s.check(ok), "{} doğru kabul edilmeli", ok);
        }
        for bad in ["merhabaa", "güzeel", "ödevvi", "unutmaa"] {
            assert!(!s.check(bad), "{} yanlış olmalı", bad);
        }
        let sug = s.suggest("güzeel", 5);
        eprintln!("öneriler: {:?}", sug);
        assert!(sug.iter().any(|x| x == "güzel"));
    }
}
