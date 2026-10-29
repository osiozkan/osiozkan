//! osisub-doc: arayüzden bağımsız belge modeli — seçim, geri al/yinele, komutlar, makro entegrasyonu.
//!
//! GUI (ve testler) yalnızca bu katmanla konuşur; böylece tüm düzenleme mantığı başsız test edilebilir.

pub mod worker;

use osisub_core::ass::{Dialogue, Entry, Script};
use osisub_core::tags;
use std::path::PathBuf;

#[derive(Clone, Debug)]
enum Change {
    Replace { at: usize, old: Entry, new: Entry },
    Insert { at: usize, items: Vec<Entry> },
    Remove { at: usize, items: Vec<Entry> },
    All { old: Vec<Entry>, new: Vec<Entry> },
}

#[derive(Clone, Debug)]
struct Txn {
    desc: String,
    key: Option<String>,
    changes: Vec<Change>,
    sel_before: (Vec<usize>, usize),
    sel_after: (Vec<usize>, usize),
}

pub struct Document {
    pub script: Script,
    pub path: Option<PathBuf>,
    /// Seçili entry indeksleri (0 tabanlı, sıralı).
    pub selection: Vec<usize>,
    pub active: usize,
    undo: Vec<Txn>,
    redo: Vec<Txn>,
    /// Kaydedildiği andaki geri-al yığın uzunluğu; `None` = kaydedilmiş duruma geri dönülemez.
    saved_at: Option<usize>,
    pub max_undo: usize,
    /// Her içerik değişikliğinde artar (önbellekleri geçersiz kılmak için).
    pub rev: u64,
}

/// `edit` içinde kullanılan kayıt tutan düzenleyici.
pub struct Tx<'a> {
    script: &'a mut Script,
    changes: &'a mut Vec<Change>,
}

impl Tx<'_> {
    pub fn script(&self) -> &Script {
        self.script
    }
    pub fn replace(&mut self, at: usize, new: Entry) {
        let old = std::mem::replace(&mut self.script.entries[at], new.clone());
        if old != new {
            self.changes.push(Change::Replace { at, old, new });
        }
    }
    pub fn insert(&mut self, at: usize, items: Vec<Entry>) {
        if items.is_empty() {
            return;
        }
        let at = at.min(self.script.entries.len());
        for (k, e) in items.iter().enumerate() {
            self.script.entries.insert(at + k, e.clone());
        }
        self.changes.push(Change::Insert { at, items });
    }
    pub fn remove(&mut self, at: usize, count: usize) {
        let end = (at + count).min(self.script.entries.len());
        if at >= end {
            return;
        }
        let items: Vec<Entry> = self.script.entries.drain(at..end).collect();
        self.changes.push(Change::Remove { at, items });
    }
    pub fn dialogue_mut(&mut self, at: usize, f: impl FnOnce(&mut Dialogue)) {
        if let Entry::Dialogue(d) = &self.script.entries[at] {
            let mut nd = d.clone();
            f(&mut nd);
            self.replace(at, Entry::Dialogue(nd));
        }
    }
}

impl Document {
    pub fn new(script: Script) -> Document {
        Document { script, path: None, selection: vec![], active: 0, undo: vec![], redo: vec![], saved_at: Some(0), max_undo: 500, rev: 0 }
    }

    pub fn open(path: impl Into<PathBuf>) -> Result<Document, String> {
        let path = path.into();
        let mut d = Document::new(osisub_core::io::load(&path)?);
        d.path = Some(path);
        Ok(d)
    }

    pub fn save(&mut self) -> Result<(), String> {
        let p = self.path.clone().ok_or("dosya yolu yok")?;
        self.save_as(p)
    }

    pub fn save_as(&mut self, path: PathBuf) -> Result<(), String> {
        osisub_core::io::save(&self.script, &path)?;
        self.path = Some(path);
        self.saved_at = Some(self.undo.len());
        Ok(())
    }

    pub fn is_dirty(&self) -> bool {
        self.saved_at != Some(self.undo.len())
    }

    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }
    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }
    pub fn undo_desc(&self) -> Option<&str> {
        self.undo.last().map(|t| t.desc.as_str())
    }

    fn sel_state(&self) -> (Vec<usize>, usize) {
        (self.selection.clone(), self.active)
    }

    /// Bir işlem grubu (tek geri-al adımı). Değişiklik olmazsa kayıt eklenmez.
    pub fn edit<R>(&mut self, desc: &str, f: impl FnOnce(&mut Tx) -> R) -> R {
        self.edit_keyed(desc, None, f)
    }

    /// Aynı `key` ile art arda gelen düzenlemeler (ör. yazma) tek geri-al adımına birleşir.
    pub fn edit_keyed<R>(&mut self, desc: &str, key: Option<&str>, f: impl FnOnce(&mut Tx) -> R) -> R {
        let sel_before = self.sel_state();
        let mut changes = Vec::new();
        let r = f(&mut Tx { script: &mut self.script, changes: &mut changes });
        if changes.is_empty() {
            return r;
        }
        self.redo.clear();
        self.rev += 1;
        // Kaydedilmiş duruma dönüş artık mümkün değilse işaretle.
        if let Some(s) = self.saved_at {
            if s > self.undo.len() {
                self.saved_at = None;
            }
        }
        let sel_after = self.sel_state();
        let merge = match (self.undo.last(), key) {
            (Some(last), Some(k)) => last.key.as_deref() == Some(k) && self.saved_at != Some(self.undo.len()),
            _ => false,
        };
        if merge {
            let last = self.undo.last_mut().unwrap();
            last.changes.extend(changes);
            last.sel_after = sel_after;
        } else {
            self.undo.push(Txn { desc: desc.to_string(), key: key.map(String::from), changes, sel_before, sel_after });
            if self.undo.len() > self.max_undo {
                self.undo.remove(0);
                self.saved_at = self.saved_at.and_then(|s| s.checked_sub(1));
            }
        }
        r
    }

    fn apply_inverse(script: &mut Script, c: &Change) {
        match c {
            Change::Replace { at, old, .. } => script.entries[*at] = old.clone(),
            Change::Insert { at, items } => {
                script.entries.drain(*at..*at + items.len());
            }
            Change::Remove { at, items } => {
                for (k, e) in items.iter().enumerate() {
                    script.entries.insert(*at + k, e.clone());
                }
            }
            Change::All { old, .. } => script.entries = old.clone(),
        }
    }

    fn apply_forward(script: &mut Script, c: &Change) {
        match c {
            Change::Replace { at, new, .. } => script.entries[*at] = new.clone(),
            Change::Insert { at, items } => {
                for (k, e) in items.iter().enumerate() {
                    script.entries.insert(*at + k, e.clone());
                }
            }
            Change::Remove { at, items } => {
                script.entries.drain(*at..*at + items.len());
            }
            Change::All { new, .. } => script.entries = new.clone(),
        }
    }

    pub fn undo(&mut self) -> Option<String> {
        let t = self.undo.pop()?;
        for c in t.changes.iter().rev() {
            Self::apply_inverse(&mut self.script, c);
        }
        self.rev += 1;
        (self.selection, self.active) = t.sel_before.clone();
        let d = t.desc.clone();
        self.redo.push(t);
        Some(d)
    }

    pub fn redo(&mut self) -> Option<String> {
        let t = self.redo.pop()?;
        for c in t.changes.iter() {
            Self::apply_forward(&mut self.script, c);
        }
        self.rev += 1;
        (self.selection, self.active) = t.sel_after.clone();
        let d = t.desc.clone();
        self.undo.push(t);
        Some(d)
    }

    /// `after` ile mevcut betik arasındaki farkı tek geri-al adımı olarak uygular.
    pub fn apply_new_script(&mut self, desc: &str, after: Script) {
        let before = self.script.entries.clone();
        if before == after.entries && self.script.crlf == after.crlf {
            return;
        }
        let new_entries = after.entries;
        self.edit(desc, |tx| {
            if before.len() == new_entries.len() {
                for (i, e) in new_entries.into_iter().enumerate() {
                    if before[i] != e {
                        tx.replace(i, e);
                    }
                }
            } else {
                tx.changes.push(Change::All { old: before, new: new_entries.clone() });
                tx.script.entries = new_entries;
            }
        });
    }

    // ------------------------------------------------------------ seçim

    pub fn select(&mut self, mut idx: Vec<usize>, active: usize) {
        idx.sort_unstable();
        idx.dedup();
        idx.retain(|&i| i < self.script.entries.len());
        self.selection = idx;
        self.active = active;
    }

    /// Seçili diyalog satırlarının entry indeksleri; seçim yoksa etkin satır.
    pub fn target_lines(&self) -> Vec<usize> {
        let mut v: Vec<usize> = self.selection.iter().copied().filter(|&i| matches!(self.script.entries.get(i), Some(Entry::Dialogue(_)))).collect();
        if v.is_empty() && matches!(self.script.entries.get(self.active), Some(Entry::Dialogue(_))) {
            v.push(self.active);
        }
        v
    }

    // ------------------------------------------------------------ komutlar

    pub fn set_text(&mut self, at: usize, text: &str) {
        self.edit_keyed("Metin düzenle", Some(&format!("text:{}", at)), |tx| tx.dialogue_mut(at, |d| d.text = text.to_string()));
    }

    pub fn set_times(&mut self, at: usize, start: i64, end: i64) {
        self.edit_keyed("Zaman düzenle", Some(&format!("time:{}", at)), |tx| {
            tx.dialogue_mut(at, |d| {
                d.start_time = start.max(0);
                d.end_time = end.max(0);
            })
        });
    }

    pub fn shift_selected(&mut self, ms: i64) {
        let t = self.target_lines();
        self.edit("Kaydır", |tx| {
            for i in t {
                tx.dialogue_mut(i, |d| {
                    d.start_time = (d.start_time + ms).max(0);
                    d.end_time = (d.end_time + ms).max(0);
                })
            }
        });
    }

    pub fn set_style_selected(&mut self, style: &str) {
        let t = self.target_lines();
        self.edit("Stil değiştir", |tx| {
            for i in t {
                tx.dialogue_mut(i, |d| d.style = style.to_string())
            }
        });
    }

    /// Etkin satırdan sonra yeni satır ekler (süre: 2 sn, etkin satır bitişinden başlar). Yeni satırı seçer.
    pub fn insert_after_active(&mut self) -> usize {
        let at = self.active.min(self.script.entries.len().saturating_sub(1));
        let (start, style) = match self.script.entries.get(at) {
            Some(Entry::Dialogue(d)) => (d.end_time, d.style.clone()),
            _ => (self.script.dialogues().map(|d| d.end_time).max().unwrap_or(0), "Default".into()),
        };
        let pos = if matches!(self.script.entries.get(at), Some(Entry::Dialogue(_))) { at + 1 } else { self.script.entries.len() };
        let d = Dialogue { start_time: start, end_time: start + 2000, style, ..Default::default() };
        self.edit("Satır ekle", |tx| tx.insert(pos, vec![Entry::Dialogue(d)]));
        self.select(vec![pos], pos);
        pos
    }

    pub fn duplicate_selected(&mut self) {
        let t = self.target_lines();
        let Some(&last) = t.last() else { return };
        let copies: Vec<Entry> = t.iter().map(|&i| self.script.entries[i].clone()).collect();
        let n = copies.len();
        self.edit("Çoğalt", |tx| tx.insert(last + 1, copies));
        self.select((last + 1..last + 1 + n).collect(), last + 1);
    }

    pub fn delete_selected(&mut self) {
        let mut t = self.target_lines();
        if t.is_empty() {
            return;
        }
        t.sort_unstable();
        let first = t[0];
        self.edit("Sil", |tx| {
            for &i in t.iter().rev() {
                tx.remove(i, 1);
            }
        });
        let n = self.script.entries.len();
        let a = first.min(n.saturating_sub(1));
        self.select(if n > 0 { vec![a] } else { vec![] }, a);
    }

    /// Satırı metindeki `char_pos` karakterinden ve `time_ms` zamanından ikiye böler.
    /// `char_pos` etiketsiz (görünür+etiket dahil ham) metin üzerindedir; etiket bloklarının ortasında bölünmez.
    pub fn split(&mut self, at: usize, char_pos: usize, time_ms: i64) -> bool {
        let Some(Entry::Dialogue(d)) = self.script.entries.get(at).cloned() else { return false };
        let chars: Vec<char> = d.text.chars().collect();
        if char_pos == 0 || char_pos >= chars.len() || time_ms <= d.start_time || time_ms >= d.end_time {
            return false;
        }
        // Etiket bloğu içindeyse engelle.
        let before: String = chars[..char_pos].iter().collect();
        if before.matches('{').count() != before.matches('}').count() {
            return false;
        }
        let (a, b): (String, String) = (before.trim_end().to_string(), chars[char_pos..].iter().collect::<String>().trim_start().to_string());
        let d1 = Dialogue { text: a, end_time: time_ms, ..d.clone() };
        let d2 = Dialogue { text: b, start_time: time_ms, ..d };
        self.edit("Böl", |tx| {
            tx.replace(at, Entry::Dialogue(d1));
            tx.insert(at + 1, vec![Entry::Dialogue(d2)]);
        });
        self.select(vec![at, at + 1], at);
        true
    }

    /// Ardışık seçili satırları tek satırda birleştirir (metinler `\N` ile).
    pub fn merge_selected(&mut self) -> bool {
        let t = self.target_lines();
        if t.len() < 2 || t.windows(2).any(|w| w[1] != w[0] + 1) {
            return false;
        }
        let ds: Vec<Dialogue> = t.iter().filter_map(|&i| if let Entry::Dialogue(d) = &self.script.entries[i] { Some(d.clone()) } else { None }).collect();
        let mut m = ds[0].clone();
        m.end_time = ds.iter().map(|d| d.end_time).max().unwrap();
        m.text = ds.iter().map(|d| d.text.trim()).filter(|s| !s.is_empty()).collect::<Vec<_>>().join("\\N");
        let first = t[0];
        self.edit("Birleştir", |tx| {
            tx.replace(first, Entry::Dialogue(m));
            tx.remove(first + 1, t.len() - 1);
        });
        self.select(vec![first], first);
        true
    }

    /// Satır metnini `balance_lines` ile dengeler.
    pub fn balance_selected(&mut self, max_len: usize) {
        let t = self.target_lines();
        self.edit("Satırları dengele", |tx| {
            for i in t {
                tx.dialogue_mut(i, |d| {
                    if !d.text.contains('{') {
                        d.text = osisub_core::qc::balance_lines(&d.text, max_len);
                    }
                })
            }
        });
    }

    /// Seçili satırların yorum durumunu çevirir (hepsi yorumsa yorumu kaldırır).
    pub fn toggle_comment_selected(&mut self) {
        let t = self.target_lines();
        let all_comments = t.iter().all(|&i| matches!(&self.script.entries[i], Entry::Dialogue(d) if d.comment));
        self.edit(if all_comments { "Yorumu kaldır" } else { "Yorum satırı yap" }, |tx| {
            for i in t {
                tx.dialogue_mut(i, |d| d.comment = !all_comments)
            }
        });
    }

    /// Etkin satırdan önce yeni satır ekler.
    pub fn insert_before_active(&mut self) -> usize {
        let at = self.active.min(self.script.entries.len());
        let (end, style) = match self.script.entries.get(at) {
            Some(Entry::Dialogue(d)) => (d.start_time, d.style.clone()),
            _ => (0, "Default".into()),
        };
        let d = Dialogue { start_time: (end - 2000).max(0), end_time: end.max(1), style, ..Default::default() };
        self.edit("Satır ekle", |tx| tx.insert(at, vec![Entry::Dialogue(d)]));
        self.select(vec![at], at);
        at
    }

    /// Seçili satırları ASS metni olarak verir (panoya kopyalamak için).
    pub fn copy_lines(&self) -> String {
        self.target_lines()
            .iter()
            .filter_map(|&i| if let Entry::Dialogue(d) = &self.script.entries[i] { Some(osisub_core::ass::dialogue_line(d)) } else { None })
            .collect::<Vec<_>>()
            .join("\n")
    }

    pub fn cut_lines(&mut self) -> String {
        let t = self.copy_lines();
        self.delete_selected();
        t
    }

    /// Panodaki metni etkin/seçili son satırdan sonra yapıştırır. `Dialogue:` satırları olduğu gibi,
    /// düz metin satırları ise art arda yeni altyazı satırları olarak eklenir. Eklenen sayıyı döndürür.
    pub fn paste_lines(&mut self, text: &str) -> usize {
        let parsed: Vec<Entry> = Script::parse(&format!("[Events]\n{}", text.replace("\r\n", "\n"))).entries.into_iter().filter(|e| matches!(e, Entry::Dialogue(_))).collect();
        let items: Vec<Entry> = if !parsed.is_empty() {
            parsed
        } else {
            let mut t = self.script.dialogues().map(|d| d.end_time).max().unwrap_or(0);
            let style = match self.script.entries.get(self.active) {
                Some(Entry::Dialogue(d)) => d.style.clone(),
                _ => "Default".into(),
            };
            text.lines()
                .filter(|l| !l.trim().is_empty())
                .map(|l| {
                    let d = Dialogue { start_time: t, end_time: t + 2000, style: style.clone(), text: l.trim().replace('\n', "\\N"), ..Default::default() };
                    t += 2000;
                    Entry::Dialogue(d)
                })
                .collect()
        };
        if items.is_empty() {
            return 0;
        }
        let at = self.target_lines().last().map(|i| i + 1).unwrap_or(self.script.entries.len());
        let n = items.len();
        self.edit("Yapıştır", |tx| tx.insert(at, items));
        self.select((at..at + n).collect(), at);
        n
    }

    pub fn stripped_text(&self, at: usize) -> String {
        match self.script.entries.get(at) {
            Some(Entry::Dialogue(d)) => tags::visible_text(&d.text),
            _ => String::new(),
        }
    }

    // ------------------------------------------------------------ Lua

    /// Arka plan çalışanından gelen sonucu uygular (tek geri-al adımı).
    pub fn apply_run_done(&mut self, done: worker::RunDone, fallback_name: &str) {
        if done.cancelled {
            return;
        }
        let desc = done.undo_desc.unwrap_or_else(|| fallback_name.to_string());
        self.apply_new_script(&desc, done.script);
        if let Some(s) = done.selection {
            self.select(s, done.active.unwrap_or(self.active));
        } else if let Some(a) = done.active {
            self.active = a;
        }
    }

    /// Makroyu çalıştırır; sonucu tek geri-al adımı olarak uygular. Seçim/etkin satır makronun
    /// döndürdüğüne göre güncellenir. Dönüş: (günlük metni, iptal edildi mi).
    pub fn run_macro(&mut self, engine: &mut osisub_lua::Engine, id: usize) -> Result<(String, bool), String> {
        let name = engine.macros().get(id).map(|m| m.name.clone()).unwrap_or_default();
        engine.set_script(self.script.clone());
        let sel: Vec<usize> = self.selection.iter().map(|i| i + 1).collect();
        let active = self.active + 1;
        let res = engine.run_macro(id, &sel, active);
        let log = engine.take_log();
        match res {
            Err(e) => Err(format!("{}{}", log, e)),
            Ok(out) if out.cancelled => Ok((log, true)),
            Ok(out) => {
                let after = engine.take_script();
                let desc = out.undo_desc.unwrap_or(name);
                self.apply_new_script(&desc, after);
                if let Some(s) = out.selection {
                    self.select(s.into_iter().filter(|&i| i >= 1).map(|i| i - 1).collect(), out.active.unwrap_or(active).saturating_sub(1));
                } else if let Some(a) = out.active {
                    self.active = a.saturating_sub(1);
                }
                Ok((log, false))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn doc() -> Document {
        let ds = vec![
            Dialogue { start_time: 0, end_time: 2000, text: "bir".into(), ..Default::default() },
            Dialogue { start_time: 2000, end_time: 4000, text: "iki".into(), ..Default::default() },
            Dialogue { start_time: 4000, end_time: 6000, text: "üç".into(), ..Default::default() },
        ];
        Document::new(Script::from_dialogues(ds))
    }

    fn first_dlg(d: &Document) -> usize {
        d.script.entries.iter().position(|e| matches!(e, Entry::Dialogue(_))).unwrap()
    }

    #[test]
    fn undo_redo_and_dirty() {
        let mut d = doc();
        let a = first_dlg(&d);
        assert!(!d.is_dirty());
        d.set_text(a, "x");
        assert!(d.is_dirty());
        d.undo();
        assert!(!d.is_dirty());
        assert_eq!(d.stripped_text(a), "bir");
        d.redo();
        assert_eq!(d.stripped_text(a), "x");
    }

    #[test]
    fn typing_coalesces() {
        let mut d = doc();
        let a = first_dlg(&d);
        for t in ["b", "bi", "bir!"] {
            d.set_text(a, t);
        }
        d.undo();
        assert_eq!(d.stripped_text(a), "bir");
        assert!(!d.can_undo());
    }

    #[test]
    fn split_merge_delete() {
        let mut d = doc();
        let a = first_dlg(&d);
        d.select(vec![a], a);
        d.set_text(a, "merhaba dünya");
        assert!(d.split(a, 8, 1000));
        assert_eq!(d.script.dialogues().count(), 4);
        d.select(vec![a, a + 1], a);
        assert!(d.merge_selected());
        assert_eq!(d.script.dialogues().count(), 3);
        d.select(vec![a], a);
        d.delete_selected();
        assert_eq!(d.script.dialogues().count(), 2);
        d.undo();
        d.undo();
        d.undo();
        d.undo();
        assert_eq!(d.stripped_text(a), "bir");
        assert_eq!(d.script.dialogues().count(), 3);
    }

    #[test]
    fn clipboard_roundtrip() {
        let mut d = doc();
        let a = first_dlg(&d);
        d.select(vec![a, a + 1], a);
        let clip = d.copy_lines();
        assert_eq!(clip.lines().count(), 2);
        d.select(vec![a + 2], a + 2);
        assert_eq!(d.paste_lines(&clip), 2);
        assert_eq!(d.script.dialogues().count(), 5);
        assert_eq!(d.stripped_text(a + 3), "bir");
        // düz metin yapıştırma
        assert_eq!(d.paste_lines("yeni bir\nikinci satır"), 2);
        assert_eq!(d.script.dialogues().count(), 7);
        d.toggle_comment_selected();
        assert!(d.script.dialogues().filter(|x| x.comment).count() == 2);
        let cut = d.cut_lines();
        assert_eq!(cut.lines().count(), 2);
        assert_eq!(d.script.dialogues().count(), 5);
    }

    #[test]
    fn macro_is_one_undo_step() {
        use osisub_lua::{Engine, HeadlessHost};
        let mut e = Engine::new(Box::new(HeadlessHost { quiet: true, ..Default::default() })).unwrap();
        e.load_script(std::path::Path::new("../osisub-lua/tests/fixtures/sample.lua")).unwrap();
        let mut d = doc();
        let a = first_dlg(&d);
        d.select(vec![a, a + 1], a);
        let (_, cancelled) = d.run_macro(&mut e, 0).unwrap();
        assert!(!cancelled);
        assert_eq!(d.stripped_text(a), "bir!");
        assert_eq!(d.undo_desc(), Some("kaydir"));
        d.undo();
        assert_eq!(d.stripped_text(a), "bir");
        assert!(!d.can_undo());
    }
}
