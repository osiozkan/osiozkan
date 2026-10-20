//! Spell checking with Hunspell dictionaries (pure-Rust `spellbook`), Turkish first.
//!
//! Dictionaries are not bundled: `download` fetches them from the LibreOffice
//! dictionaries repository into the user's config folder.
use std::collections::{BTreeMap, HashSet};
use std::path::{Path, PathBuf};
use asscore::tags::{self, Part};

/// Same folder rule as the GUI settings (`~/.config/yuzu`).
pub fn config_dir() -> PathBuf {
    if let Some(x) = std::env::var_os("YUZU_CONFIG") { return PathBuf::from(x); }
    if let Some(x) = std::env::var_os("XDG_CONFIG_HOME") { return PathBuf::from(x).join("yuzu"); }
    if let Some(h) = std::env::var_os("HOME") { return PathBuf::from(h).join(".config/yuzu"); }
    if let Some(a) = std::env::var_os("APPDATA") { return PathBuf::from(a).join("Yuzu"); }
    PathBuf::from(".yuzu")
}

fn tr_lower(s: &str) -> String {
    s.chars().flat_map(|c| match c { 'I' => vec!['ı'], 'İ' => vec!['i'], c => c.to_lowercase().collect() }).collect()
}

pub fn dict_dir() -> PathBuf {
    config_dir().join("dictionaries")
}

pub fn available(lang: &str) -> bool {
    dict_dir().join(format!("{lang}.aff")).exists() && dict_dir().join(format!("{lang}.dic")).exists()
}

/// `spellbook` rejects flag 0 (used by the numeric-flag Turkish dictionary). Shift every
/// numeric flag by one in both files when `FLAG num` is used and a 0 flag occurs.
pub fn patch_numeric_flags(aff: &str, dic: &str) -> (String, String) {
    let numeric = aff.lines().any(|l| l.trim() == "FLAG num");
    let has_zero = aff.lines().any(|l| l.starts_with("SFX 0 ") || l.starts_with("PFX 0 "));
    if !numeric || !has_zero {
        return (aff.to_string(), dic.to_string());
    }
    let mut a = String::with_capacity(aff.len());
    for line in aff.lines() {
        if line.starts_with("SFX ") || line.starts_with("PFX ") {
            let mut parts = line.splitn(3, ' ');
            let (kind, flag, rest) = (parts.next().unwrap(), parts.next().unwrap_or(""), parts.next().unwrap_or(""));
            match flag.parse::<u32>() {
                Ok(f) => {
                    a.push_str(&format!("{kind} {} {rest}\n", f + 1));
                    continue;
                }
                Err(_) => {}
            }
        }
        a.push_str(line);
        a.push('\n');
    }
    let mut d = String::with_capacity(dic.len());
    for (i, line) in dic.lines().enumerate() {
        if i == 0 {
            d.push_str(line);
            d.push('\n');
            continue;
        }
        match line.split_once('/') {
            Some((w, flags)) => {
                d.push_str(w);
                d.push('/');
                let shifted: Vec<String> = flags.split(',').map(|f| f.trim().parse::<u32>().map(|n| (n + 1).to_string()).unwrap_or_else(|_| f.to_string())).collect();
                d.push_str(&shifted.join(","));
                d.push('\n');
            }
            None => {
                d.push_str(line);
                d.push('\n');
            }
        }
    }
    (a, d)
}

/// Download `<lang>` (e.g. "tr_TR") from LibreOffice's dictionary repo and install it.
pub fn download(lang: &str) -> Result<(), String> {
    let folder = match lang {
        "en_US" | "en_GB" => "en",
        other => other,
    };
    let base = format!("https://raw.githubusercontent.com/LibreOffice/dictionaries/master/{folder}");
    let get = |name: &str| -> Result<String, String> {
        let url = format!("{base}/{lang}.{name}");
        let mut resp = ureq::get(&url).call().map_err(|e| format!("{url}: {e}"))?;
        resp.body_mut().with_config().limit(200 * 1024 * 1024).read_to_string().map_err(|e| e.to_string())
    };
    let aff = get("aff")?;
    let dic = get("dic")?;
    let (aff, dic) = patch_numeric_flags(&aff, &dic);
    let d = dict_dir();
    std::fs::create_dir_all(&d).map_err(|e| e.to_string())?;
    std::fs::write(d.join(format!("{lang}.aff")), aff).map_err(|e| e.to_string())?;
    std::fs::write(d.join(format!("{lang}.dic")), dic).map_err(|e| e.to_string())?;
    Ok(())
}

pub struct Speller {
    dict: spellbook::Dictionary,
    user: HashSet<String>,
    user_path: PathBuf,
}

impl Speller {
    pub fn load(lang: &str) -> Result<Speller, String> {
        Self::load_from(&dict_dir(), lang)
    }

    pub fn load_from(dir: &Path, lang: &str) -> Result<Speller, String> {
        let aff = std::fs::read_to_string(dir.join(format!("{lang}.aff"))).map_err(|e| format!("{lang}.aff: {e}"))?;
        let dic = std::fs::read_to_string(dir.join(format!("{lang}.dic"))).map_err(|e| format!("{lang}.dic: {e}"))?;
        let (aff, dic) = patch_numeric_flags(&aff, &dic);
        let dict = spellbook::Dictionary::new(&aff, &dic).map_err(|e| format!("dictionary: {e}"))?;
        let user_path = dir.join("user_words.txt");
        let user = std::fs::read_to_string(&user_path).map(|s| s.lines().map(|l| l.trim().to_lowercase()).filter(|l| !l.is_empty()).collect()).unwrap_or_default();
        Ok(Speller { dict, user, user_path })
    }

    pub fn add_user_word(&mut self, w: &str) {
        let w = w.trim().to_lowercase();
        if self.user.insert(w.clone()) {
            let mut all: Vec<&String> = self.user.iter().collect();
            all.sort();
            let _ = std::fs::write(&self.user_path, all.iter().map(|s| s.as_str()).collect::<Vec<_>>().join("\n"));
        }
    }

    /// Turkish-aware check: handles `Ahmet'in` style suffixes and proper nouns.
    pub fn check(&self, word: &str) -> bool {
        if word.is_empty() || word.chars().all(|c| !c.is_alphabetic()) {
            return true;
        }
        if self.user.contains(&tr_lower(word)) {
            return true;
        }
        if self.dict.check(word) {
            return true;
        }
        let lower = tr_lower(word);
        if lower != word && self.dict.check(&lower) {
            return true;
        }
        if let Some(idx) = word.find(['\'', '’']) {
            let stem = &word[..idx];
            if stem.chars().next().map(|c| c.is_uppercase()).unwrap_or(false) {
                return true; // proper noun + suffix
            }
            return self.check(stem);
        }
        // hyphenated compounds
        if word.contains('-') {
            return word.split('-').all(|p| self.check(p));
        }
        false
    }

    pub fn suggest(&self, word: &str, max: usize) -> Vec<String> {
        let mut out = vec![];
        self.dict.suggest(word, &mut out);
        out.truncate(max);
        out
    }
}

#[derive(Debug, Clone)]
pub struct Misspelling {
    pub word: String,
    pub events: Vec<usize>,
}

/// Words of the visible text (tag-aware), with `'`/`’` kept inside words.
pub fn words(text: &str) -> Vec<String> {
    let mut out = vec![];
    for seg in tags::split_parts(text) {
        if let Part::Text(t) = seg {
            let t = t.replace("\\N", " ").replace("\\n", " ").replace("\\h", " ");
            let mut cur = String::new();
            for c in t.chars() {
                if c.is_alphabetic() || ((c == '\'' || c == '’' || c == '-') && !cur.is_empty()) {
                    cur.push(c);
                } else if !cur.is_empty() {
                    out.push(cur.trim_end_matches(['\'', '’', '-']).to_string());
                    cur.clear();
                }
            }
            if !cur.is_empty() {
                out.push(cur.trim_end_matches(['\'', '’', '-']).to_string());
            }
        }
    }
    out.retain(|w| !w.is_empty());
    out
}

/// Misspelled words (grouped, most frequent first) across dialogue lines.
pub fn check_script(sp: &Speller, events: &[asscore::Event], ignore: &HashSet<String>) -> Vec<Misspelling> {
    let mut map: BTreeMap<String, Vec<usize>> = BTreeMap::new();
    let mut cache: std::collections::HashMap<String, bool> = Default::default();
    for (i, e) in events.iter().enumerate() {
        if e.comment || e.effect.to_ascii_lowercase().contains("karaoke") {
            continue;
        }
        for w in words(&e.text) {
            if ignore.contains(&w) {
                continue;
            }
            let ok = *cache.entry(w.clone()).or_insert_with(|| sp.check(&w));
            if !ok {
                map.entry(w).or_default().push(i);
            }
        }
    }
    let mut v: Vec<Misspelling> = map.into_iter().map(|(word, mut events)| {
        events.dedup();
        Misspelling { word, events }
    }).collect();
    v.sort_by(|a, b| b.events.len().cmp(&a.events.len()).then(a.word.cmp(&b.word)));
    v
}

/// Replace whole-word occurrences in visible text only.
pub fn replace_word(text: &str, from: &str, to: &str) -> String {
    let mut segs = tags::split_parts(text);
    for seg in segs.iter_mut() {
        if let Part::Text(t) = seg {
            let mut out = String::new();
            let chars: Vec<char> = t.chars().collect();
            let from_c: Vec<char> = from.chars().collect();
            let mut i = 0;
            while i < chars.len() {
                let is_word_char = |c: char| c.is_alphabetic() || c == '\'' || c == '’' || c == '-';
                let boundary_before = i == 0 || !is_word_char(chars[i - 1]);
                if boundary_before && chars[i..].starts_with(&from_c) && (i + from_c.len() == chars.len() || !is_word_char(chars[i + from_c.len()])) {
                    out.push_str(to);
                    i += from_c.len();
                } else {
                    out.push(chars[i]);
                    i += 1;
                }
            }
            *t = out;
        }
    }
    tags::join_parts(&segs)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flag_patch_shifts_zero() {
        let aff = "SET UTF-8\nFLAG num\n\nSFX 0 Y 1\nSFX 0 0 lar .\n\nSFX 1 Y 1\nSFX 1 0 ler .\n";
        let dic = "2\nkitap/0\nev/1,0\n";
        let (a, d) = patch_numeric_flags(aff, dic);
        assert!(a.contains("SFX 1 Y 1") && a.contains("SFX 2 0 ler ."));
        assert_eq!(d, "2\nkitap/1\nev/2,1\n");
        // untouched when no flag 0
        let (a2, _) = patch_numeric_flags("FLAG num\nSFX 1 Y 1\n", "1\nx/1\n");
        assert!(a2.starts_with("FLAG num\nSFX 1 Y 1"));
    }

    #[test]
    fn tokenizing_and_replacing() {
        assert_eq!(words("{\\i1}Ahmet'in kitabı{\\i0}\\Nçok-güzel, değil mi?"), vec!["Ahmet'in", "kitabı", "çok-güzel", "değil", "mi"]);
        assert_eq!(replace_word("{\\b1}guzel{\\b0} guzeller guzel", "guzel", "güzel"), "{\\b1}güzel{\\b0} guzeller güzel");
    }

    /// Run with `YUZU_DICT_DIR=/path/with/tr_TR.{aff,dic} cargo test -p yuzu-spell real_turkish -- --ignored`.
    #[test]
    #[ignore]
    fn real_turkish_dictionary() {
        let dir = PathBuf::from(std::env::var("YUZU_DICT_DIR").expect("YUZU_DICT_DIR"));
        let t = std::time::Instant::now();
        let sp = Speller::load_from(&dir, "tr_TR").unwrap();
        eprintln!("loaded in {:?}", t.elapsed());
        for ok in ["merhaba", "dünyayı", "gidiyorum", "kitaplarımızdan", "İstanbul'a", "Ahmet'in", "çok", "Bugün", "yapamıyorum", "anlamıyorsun"] {
            assert!(sp.check(ok), "{ok} should be valid");
        }
        for bad in ["güzeel", "asdfgh", "merhabaa", "kitapp"] {
            assert!(!sp.check(bad), "{bad} should be invalid");
        }
        assert!(sp.suggest("güzeel", 5).iter().any(|s| s == "güzel"));
    }

    #[test]
    fn checks_with_a_tiny_dictionary() {
        let dir = std::env::temp_dir().join(format!("yuzu-dict-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("xx.aff"), "SET UTF-8\nFLAG num\nSFX 0 Y 1\nSFX 0 0 lar .\n").unwrap();
        std::fs::write(dir.join("xx.dic"), "2\nkitap/0\nev\n").unwrap();
        let sp = Speller::load_from(&dir, "xx").unwrap();
        assert!(sp.check("kitap") && sp.check("kitaplar") && sp.check("ev"));
        assert!(!sp.check("kitapz"));
        assert!(sp.check("Ahmet'in")); // proper noun + suffix
        assert!(sp.check("ev'de") || !sp.check("ev'de")); // just must not panic
        let evs = vec![asscore::Event::new(0, 1, "Default", "kitaplar evv kitap")];
        let m = check_script(&sp, &evs, &HashSet::new());
        assert_eq!(m.len(), 1);
        assert_eq!(m[0].word, "evv");
    }
}
