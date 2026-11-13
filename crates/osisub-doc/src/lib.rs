//! osisub-doc: arayüzden bağımsız belge modeli — seçim, geri al/yinele, komutlar, makro entegrasyonu.
//!
//! GUI (ve testler) yalnızca bu katmanla konuşur; böylece tüm düzenleme mantığı başsız test edilebilir.

pub mod sysclip;
pub mod worker;

use osisub_core::ass::{Dialogue, Entry, Script};
use osisub_core::tags;
use std::path::PathBuf;

#[derive(Clone, Debug)]
enum Change {
    Replace { at: usize, old: Entry, new: Entry },
    Insert { at: usize, items: Vec<Entry> },
    Remove { at: usize, items: Vec<Entry> },
}

#[derive(Clone, Debug)]
struct Txn {
    desc: String,
    key: Option<String>,
    changes: Vec<Change>,
    sel_before: (Vec<usize>, usize),
    sel_after: (Vec<usize>, usize),
}

/// Satır birleştirme türü.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum MergeMode {
    /// metinler `\N` ile art arda
    Concat,
    /// yalnızca ilk satırın metni
    KeepFirst,
    /// her satır bir karaoke hecesi (`\k` = satır süresi)
    Karaoke,
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
        self.script.entries.splice(at..at, items.iter().cloned());
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

/// Her yeni belge farklı bir `rev` aralığından başlar: arayüz önbellekleri (ızgara satırları, QC,
/// yazım) `rev` ile anahtarlanır; iki belgenin ikisi de 0'dan başlarsa yeni açılan belge eskisinin
/// önbelleğiyle gösteriliyordu (HATA: ikinci dosyayı açınca ızgara boş/yanlış kalıyordu).
static NEXT_REV_BASE: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

impl Document {
    pub fn new(script: Script) -> Document {
        let rev = NEXT_REV_BASE.fetch_add(1 << 32, std::sync::atomic::Ordering::Relaxed);
        Document { script, path: None, selection: vec![], active: 0, undo: vec![], redo: vec![], saved_at: Some(0), max_undo: 500, rev }
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

    /// Belgeyi "kaydedilmemiş" işaretler (ör. otomatik kayıttan kurtarılan içerik).
    pub fn mark_dirty(&mut self) {
        self.saved_at = None;
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

    /// Geçmiş: (geri alınabilir adımlar eskiden yeniye, yinelenebilir adımlar yakından uzağa).
    pub fn history(&self) -> (Vec<&str>, Vec<&str>) {
        (self.undo.iter().map(|t| t.desc.as_str()).collect(), self.redo.iter().rev().map(|t| t.desc.as_str()).collect())
    }

    /// Geçmişte `n` adım uygulanmış duruma gider (n < geri-al sayısı → geri al, büyükse yinele).
    pub fn goto_history(&mut self, n: usize) {
        while self.undo.len() > n && self.undo().is_some() {}
        while self.undo.len() < n && self.redo().is_some() {}
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
            self.prune_heavy();
        }
        r
    }

    /// Büyük yapısal değişiklikler (>5000 girdi) bellek yer; en fazla 8 tanesini sakla.
    /// Eski adımlar yığının dibinden atılır, böylece tutarlılık bozulmaz.
    fn prune_heavy(&mut self) {
        const MAX_HEAVY: usize = 8;
        let heavy = |t: &Txn| {
            t.changes.iter().map(|c| match c { Change::Insert { items, .. } | Change::Remove { items, .. } => items.len(), _ => 0 }).sum::<usize>() > 5000
        };
        while self.undo.iter().filter(|t| heavy(t)).count() > MAX_HEAVY {
            let Some(pos) = self.undo.iter().position(|t| heavy(t)) else { break };
            self.undo.drain(0..=pos);
            self.saved_at = self.saved_at.and_then(|s| s.checked_sub(pos + 1));
        }
    }

    fn apply_inverse(script: &mut Script, c: &Change) {
        match c {
            Change::Replace { at, old, .. } => script.entries[*at] = old.clone(),
            Change::Insert { at, items } => {
                script.entries.drain(*at..*at + items.len());
            }
            Change::Remove { at, items } => {
                script.entries.splice(*at..*at, items.iter().cloned());
            }
        }
    }

    fn apply_forward(script: &mut Script, c: &Change) {
        match c {
            Change::Replace { at, new, .. } => script.entries[*at] = new.clone(),
            Change::Insert { at, items } => {
                script.entries.splice(*at..*at, items.iter().cloned());
            }
            Change::Remove { at, items } => {
                script.entries.drain(*at..*at + items.len());
            }
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
                // yalnızca değişen orta aralık saklanır (ortak önek/sonek): satır ekleyen/silen
                // işlemler (hareket izleme, birleştirme…) büyük belgede tam kopya tutmaz
                let p = before.iter().zip(new_entries.iter()).take_while(|(a, b)| a == b).count();
                let max_s = before.len().min(new_entries.len()) - p;
                let sfx = before.iter().rev().zip(new_entries.iter().rev()).take(max_s).take_while(|(a, b)| a == b).count();
                tx.remove(p, before.len() - p - sfx);
                tx.insert(p, new_entries[p..new_entries.len() - sfx].to_vec());
            }
        });
        if self.script.entries.len() != before.len() {
            self.remap_selection(&before);
        }
    }

    /// Satır sayısı değişen toplu işlemden sonra etkin satırı ve seçimi içeriğe göre yeniden bulur
    /// (bulunamazsa etkin satır aynı sıradaki diyaloğa yakın kalır).
    fn remap_selection(&mut self, before: &[Entry]) {
        let find = |entries: &[Entry], e: &Entry, hint: usize| -> Option<usize> {
            // önce eski konumun yakınında ara (yinelenen satırlarda doğru olanı seçmek için)
            let n = entries.len();
            for d in 0..n {
                for i in [hint.checked_add(d), hint.checked_sub(d)].into_iter().flatten() {
                    if i < n && entries[i] == *e {
                        return Some(i);
                    }
                }
                if hint + d >= n && d > hint {
                    break;
                }
            }
            None
        };
        let entries = &self.script.entries;
        let new_active = before.get(self.active).and_then(|e| find(entries, e, self.active)).unwrap_or_else(|| {
            // eski etkin satırdan önceki diyalog sayısına göre yakın bir diyalog
            let nth = before[..self.active.min(before.len())].iter().filter(|e| matches!(e, Entry::Dialogue(_))).count();
            let ds: Vec<usize> = entries.iter().enumerate().filter(|(_, e)| matches!(e, Entry::Dialogue(_))).map(|(i, _)| i).collect();
            ds.get(nth.min(ds.len().saturating_sub(1))).copied().unwrap_or(0)
        });
        // büyük seçimlerde (ör. tümü seçili) içerik araması O(n²) olur: yalnızca etkin satır korunur
        let sel: Vec<usize> = if self.selection.len() > 200 { vec![] } else { self.selection.iter().filter_map(|&i| before.get(i).and_then(|e| find(entries, e, i))).collect() };
        let sel = if sel.is_empty() { vec![new_active] } else { sel };
        self.select(sel, new_active);
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

    /// Seçili satırları böler: `karaoke` ise hecelere, değilse `\N` noktalarına (`keep_times`: parçalar
    /// aynı zamanı alır). Tek geri-al adımı; yeni satırlar seçilir. Bölünen satır sayısını döndürür.
    pub fn split_selected(&mut self, karaoke: bool, keep_times: bool) -> usize {
        let t = self.target_lines();
        let mut plan: Vec<(usize, Vec<Entry>)> = Vec::new();
        for &i in &t {
            if let Some(Entry::Dialogue(d)) = self.script.entries.get(i) {
                let parts = if karaoke { osisub_core::splitlines::split_karaoke(d) } else { osisub_core::splitlines::split_at_breaks(d, keep_times) };
                if parts.len() > 1 {
                    plan.push((i, parts.into_iter().map(Entry::Dialogue).collect()));
                }
            }
        }
        if plan.is_empty() {
            return 0;
        }
        let n = plan.len();
        // yeni seçim: sondan başa uygulanacağı için önce dizinleri hesapla
        let mut sel = Vec::new();
        let mut shift = 0usize;
        for (i, parts) in &plan {
            for k in 0..parts.len() {
                sel.push(i + shift + k);
            }
            shift += parts.len() - 1;
        }
        let desc = if karaoke { "Karaoke hecelerine böl" } else { "Satır sonlarından böl" };
        self.edit(desc, |tx| {
            for (i, parts) in plan.into_iter().rev() {
                tx.remove(i, 1);
                tx.insert(i, parts);
            }
        });
        let first = sel[0];
        self.select(sel, first);
        n
    }

    /// Seçili satırları çoğaltır, kopyaları `frames` kare kaydırır (Aegisub "Duplicate and shift by 1 frame";
    /// kare kare tipografide kullanılır). Kopyalar seçilir.
    pub fn duplicate_shift_frames(&mut self, fr: &osisub_core::vfr::Framerate, frames: i64) {
        let t = self.target_lines();
        let Some(&last) = t.last() else { return };
        let mut tmp = Script::from_dialogues(t.iter().filter_map(|&i| if let Entry::Dialogue(d) = &self.script.entries[i] { Some(d.clone()) } else { None }).collect());
        osisub_core::qc::shift_frames(&mut tmp, fr, frames, None);
        let copies: Vec<Entry> = tmp.dialogues().cloned().map(Entry::Dialogue).collect();
        let n = copies.len();
        self.edit("Çoğalt ve kare kaydır", |tx| tx.insert(last + 1, copies));
        self.select((last + 1..last + 1 + n).collect(), last + 1);
    }

    /// Etkin satırı `ms` anındaki video karesinde ikiye böler (Aegisub "Split at video frame"):
    /// ilk parça o kareden önce biter, ikincisi o karede başlar. Bölünemiyorsa `false`.
    pub fn split_at_frame(&mut self, fr: &osisub_core::vfr::Framerate, ms: i64) -> bool {
        let at = self.active;
        let Some(Entry::Dialogue(d)) = self.script.entries.get(at).cloned() else { return false };
        let b = fr.time_at_start(fr.frame_at(ms));
        if fr.frame_at_start(b) <= fr.frame_at_start(d.start_time) || b >= d.end_time {
            return false;
        }
        let mut second = d.clone();
        second.start_time = b;
        self.edit("Video karesinde böl", |tx| {
            tx.dialogue_mut(at, |x| x.end_time = b);
            tx.insert(at + 1, vec![Entry::Dialogue(second)]);
        });
        self.select(vec![at + 1], at + 1);
        true
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
        self.merge_selected_as(MergeMode::Concat)
    }

    /// Ardışık seçili satırları birleştirir (Aegisub: Join concatenate / keep first / as karaoke).
    /// Başlangıç en erken, bitiş en geç olanınki olur.
    pub fn merge_selected_as(&mut self, mode: MergeMode) -> bool {
        let t = self.target_lines();
        if t.len() < 2 || t.windows(2).any(|w| w[1] != w[0] + 1) {
            return false;
        }
        let ds: Vec<Dialogue> = t.iter().filter_map(|&i| if let Entry::Dialogue(d) = &self.script.entries[i] { Some(d.clone()) } else { None }).collect();
        let mut m = ds[0].clone();
        m.start_time = ds.iter().map(|d| d.start_time).min().unwrap();
        m.end_time = ds.iter().map(|d| d.end_time).max().unwrap();
        m.text = match mode {
            MergeMode::Concat => ds.iter().map(|d| d.text.trim()).filter(|s| !s.is_empty()).collect::<Vec<_>>().join("\\N"),
            MergeMode::KeepFirst => ds[0].text.clone(),
            MergeMode::Karaoke => {
                // her satır bir hece: süresi (ve önündeki boşluk) \k ile; etiketler atılır
                let mut out = String::new();
                let mut cur = m.start_time;
                for d in &ds {
                    if d.start_time > cur {
                        out.push_str(&format!("{{\\k{}}}", (d.start_time - cur + 5) / 10));
                    }
                    let dur = (d.end_time - d.start_time.max(cur)).max(0);
                    out.push_str(&format!("{{\\k{}}}{}", (dur + 5) / 10, osisub_core::tags::strip_tags(&d.text).trim()));
                    cur = cur.max(d.end_time);
                }
                out
            }
        };
        let desc = match mode { MergeMode::Concat => "Birleştir", MergeMode::KeepFirst => "Birleştir (ilkini koru)", MergeMode::Karaoke => "Karaoke olarak birleştir" };
        let first = t[0];
        self.edit(desc, |tx| {
            tx.replace(first, Entry::Dialogue(m));
            tx.remove(first + 1, t.len() - 1);
        });
        self.select(vec![first], first);
        true
    }

    /// Seçili satırları `linebreak::balance` ile dengeler (tipografi/karaoke satırları atlanır).
    pub fn balance_selected(&mut self, max_len: usize) {
        let t = self.target_lines();
        self.edit("Satırları dengele", |tx| {
            for i in t {
                tx.dialogue_mut(i, |d| {
                    if !osisub_core::qc::is_typeset(d) && !osisub_core::qc::is_karaoke(d) {
                        d.text = osisub_core::linebreak::balance(&d.text, max_len);
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
            // Extradata kimlikleri bu belgeye özgü: panoya yazılmaz (Aegisub da yazmaz).
            .filter_map(|&i| if let Entry::Dialogue(d) = &self.script.entries[i] { Some(osisub_core::ass::dialogue_line(&Dialogue { extra: Vec::new(), ..d.clone() })) } else { None })
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
        let parsed: Vec<Entry> = Script::parse(&format!("[Events]\n{}", text.replace("\r\n", "\n")))
            .entries
            .into_iter()
            .filter_map(|e| match e {
                Entry::Dialogue(d) => Some(Entry::Dialogue(Dialogue { extra: Vec::new(), ..d })),
                _ => None,
            })
            .collect();
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

    /// Aegisub "Paste Over": panodaki Dialogue satırlarının seçili alanlarını seçili satırların
    /// üzerine sırayla yazar. `fields`: katman, başlangıç, bitiş, stil, konuşan, sol, sağ, dikey
    /// kenar boşluğu, efekt, metin. Değişen satır sayısını döndürür (tek geri-al adımı).
    pub fn paste_over(&mut self, text: &str, fields: &[bool; 10]) -> usize {
        let src: Vec<Dialogue> = Script::parse(&format!("[Events]\n{}", text.replace("\r\n", "\n")))
            .entries
            .into_iter()
            .filter_map(|e| match e { Entry::Dialogue(d) => Some(d), _ => None })
            .collect();
        let targets: Vec<usize> = self.target_lines().into_iter().filter(|&i| matches!(self.script.entries.get(i), Some(Entry::Dialogue(_)))).collect();
        let f = *fields;
        let mut n = 0;
        self.edit("Üzerine yapıştır", |tx| {
            for (&i, s) in targets.iter().zip(src.iter()) {
                let before = tx.script().entries[i].clone();
                tx.dialogue_mut(i, |d| {
                    if f[0] { d.layer = s.layer; }
                    if f[1] { d.start_time = s.start_time; }
                    if f[2] { d.end_time = s.end_time; }
                    if f[3] { d.style = s.style.clone(); }
                    if f[4] { d.actor = s.actor.clone(); }
                    if f[5] { d.margin_l = s.margin_l; }
                    if f[6] { d.margin_r = s.margin_r; }
                    if f[7] { d.margin_t = s.margin_t; d.margin_b = s.margin_b; }
                    if f[8] { d.effect = s.effect.clone(); }
                    if f[9] { d.text = s.text.clone(); }
                });
                if tx.script().entries[i] != before { n += 1; }
            }
        });
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
    fn split_selected_lines() {
        let mut d = doc();
        let a = first_dlg(&d);
        d.edit("x", |tx| {
            tx.dialogue_mut(a, |x| x.text = "bir\\Niki".into());
            tx.dialogue_mut(a + 2, |x| x.text = "{\\k50}a{\\k150}b".into());
        });
        let before = d.script.dialogues().count();
        d.select(vec![a, a + 1, a + 2], a);
        assert_eq!(d.split_selected(false, false), 1);
        assert_eq!(d.script.dialogues().count(), before + 1);
        assert_eq!(d.selection, vec![a, a + 1]);
        d.select(vec![a + 3], a + 3);
        assert_eq!(d.split_selected(true, false), 1);
        let texts: Vec<(i64, i64, String)> = d.script.dialogues().map(|x| (x.start_time, x.end_time, x.text.clone())).collect();
        assert_eq!(texts, vec![(0, 1000, "bir".into()), (1000, 2000, "iki".into()), (2000, 4000, "iki".into()), (4000, 4500, "a".into()), (4500, 6000, "b".into())]);
        d.undo();
        d.undo();
        assert_eq!(d.script.dialogues().count(), before);
    }

    #[test]
    fn merge_modes() {
        let mk = || {
            let mut d = doc();
            let a = first_dlg(&d);
            d.select(vec![a, a + 1, a + 2], a);
            (d, a)
        };
        let txt = |d: &Document, i: usize| match &d.script.entries[i] { Entry::Dialogue(x) => (x.start_time, x.end_time, x.text.clone()), _ => (0, 0, String::new()) };
        let (mut d, a) = mk();
        assert!(d.merge_selected_as(MergeMode::KeepFirst));
        assert_eq!(txt(&d, a), (0, 6000, "bir".into()));
        let (mut d, a) = mk();
        assert!(d.merge_selected_as(MergeMode::Karaoke));
        assert_eq!(txt(&d, a), (0, 6000, "{\\k200}bir{\\k200}iki{\\k200}üç".into()));
        assert_eq!(d.script.dialogues().count(), 1);
    }

    #[test]
    fn history_jump() {
        let mut d = doc();
        let a = first_dlg(&d);
        d.set_text(a, "x");
        d.set_text(a + 1, "y");
        d.set_text(a + 2, "z");
        let (u, r) = d.history();
        assert_eq!((u.len(), r.len()), (3, 0));
        d.goto_history(1);
        assert_eq!(d.history().0.len(), 1);
        assert_eq!(d.history().1.len(), 2);
        let txt = |d: &Document, i: usize| match &d.script.entries[i] { Entry::Dialogue(x) => x.text.clone(), _ => String::new() };
        assert_eq!((txt(&d, a), txt(&d, a + 1)), ("x".into(), "iki".into()));
        d.goto_history(3);
        assert_eq!(txt(&d, a + 2), "z");
        d.goto_history(0);
        assert_eq!(txt(&d, a), "bir");
    }

    #[test]
    fn new_documents_have_distinct_revisions() {
        let (a, b) = (doc(), doc());
        assert_ne!(a.rev, b.rev);
    }

    #[test]
    fn bulk_change_keeps_active_line_by_content() {
        let mut d = doc();
        let a = first_dlg(&d);
        d.select(vec![a + 1, a + 2], a + 2); // "iki", "üç"; etkin "üç"
        // başa bir stil ekleyip ilk satırı silen toplu işlem
        let mut s = d.script.clone();
        let st = s.styles().next().cloned().map(|mut x| { x.name = "Yeni".into(); x }).unwrap();
        let si = s.entries.iter().position(|e| matches!(e, Entry::Style(_))).unwrap();
        s.entries.insert(si + 1, Entry::Style(st));
        let first = s.entries.iter().position(|e| matches!(e, Entry::Dialogue(_))).unwrap();
        s.entries.remove(first);
        d.apply_new_script("toplu", s);
        let txt = |i: usize| match &d.script.entries[i] { Entry::Dialogue(x) => x.text.clone(), _ => "?".into() };
        assert_eq!(txt(d.active), "üç");
        assert_eq!(d.selection.iter().map(|&i| txt(i)).collect::<Vec<_>>(), vec!["iki", "üç"]);
        // etkin satır silinirse aynı sıradaki diyaloğa düşer
        let mut s = d.script.clone();
        let act = d.active;
        s.entries.remove(act);
        d.apply_new_script("sil", s);
        assert!(matches!(d.script.entries.get(d.active), Some(Entry::Dialogue(_))));
    }

    #[test]
    fn frame_split_and_duplicate_shift() {
        let fr = osisub_core::vfr::Framerate::cfr(24.0);
        let mut d = doc();
        let a = first_dlg(&d);
        d.select(vec![a], a);
        // 1. satır 0–2000 ms; 1000 ms'deki kare 24 → sınır = 24. karenin START'ı
        assert!(d.split_at_frame(&fr, 1000));
        let ds: Vec<Dialogue> = d.script.dialogues().cloned().collect();
        assert_eq!(ds.len(), 4);
        assert_eq!(ds[0].end_time, fr.time_at_start(24));
        assert_eq!(ds[1].start_time, fr.time_at_start(24));
        assert_eq!(ds[1].text, "bir");
        assert!(!d.split_at_frame(&fr, 100_000), "satır dışında bölünmemeli");
        d.undo();
        assert_eq!(d.script.dialogues().count(), 3);
        // 1 karelik satırı çoğalt + 1 kare kaydır
        d.set_times(a, fr.time_at_start(10), fr.time_at_end(10));
        d.select(vec![a], a);
        d.duplicate_shift_frames(&fr, 1);
        let ds: Vec<Dialogue> = d.script.dialogues().cloned().collect();
        assert_eq!((fr.frame_at_start(ds[1].start_time), fr.frame_at_end(ds[1].end_time)), (11, 11));
        assert_eq!(d.selection, vec![a + 1]);
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

    /// Rastgele işlem dizileri: hepsini geri al → özgün; hepsini yinele → son durum.
    #[test]
    fn random_ops_undo_redo_roundtrip() {
        let mut seed: u64 = 0x9E3779B97F4A7C15;
        let mut rnd = move |n: usize| {
            seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            ((seed >> 33) as usize) % n.max(1)
        };
        for round in 0..30 {
            let mut d = doc();
            let original = d.script.entries.clone();
            let steps = 5 + rnd(25);
            let mut heavy = 0;
            for _ in 0..steps {
                let n = d.script.entries.len();
                let dl: Vec<usize> = (0..n).filter(|&i| matches!(d.script.entries[i], Entry::Dialogue(_))).collect();
                if dl.is_empty() {
                    break;
                }
                let a = dl[rnd(dl.len())];
                match rnd(9) {
                    0 => d.set_text(a, &format!("t{}", rnd(1000))),
                    1 => {
                        d.select(vec![a], a);
                        d.insert_after_active();
                    }
                    2 => {
                        d.select(vec![a], a);
                        d.duplicate_selected();
                    }
                    3 => {
                        if dl.len() > 1 {
                            d.select(vec![a], a);
                            d.delete_selected();
                        }
                    }
                    4 => {
                        d.select(vec![a], a);
                        d.shift_selected(rnd(2000) as i64 - 1000);
                    }
                    5 => {
                        d.select(vec![a], a);
                        d.set_text(a, "merhaba dünya nasılsın");
                        d.split(a, 7, match &d.script.entries[a] { Entry::Dialogue(x) => (x.start_time + x.end_time) / 2, _ => 0 });
                    }
                    6 => {
                        if dl.len() > 1 {
                            let pos = dl.iter().position(|&x| x == a).unwrap();
                            if pos + 1 < dl.len() && dl[pos + 1] == a + 1 {
                                d.select(vec![a, a + 1], a);
                                d.merge_selected();
                            }
                        }
                    }
                    7 => {
                        d.select(vec![a], a);
                        d.toggle_comment_selected();
                    }
                    _ => {
                        let mut s = d.script.clone();
                        s.entries.push(Entry::Dialogue(Dialogue { text: "lua".into(), ..Default::default() }));
                        d.apply_new_script("yapısal", s);
                        heavy += 1;
                    }
                }
            }
            let finalstate = d.script.entries.clone();
            let mut undone = 0;
            while d.undo().is_some() {
                undone += 1;
            }
            // Ağır (yapısal) adımlar 8'den fazlaysa eskileri budanır; aksi halde tam geri dönüş şart.
            if heavy <= 8 {
                assert_eq!(d.script.entries, original, "tur {}: geri alma özgün duruma dönmedi", round);
                let mut redone = 0;
                while d.redo().is_some() {
                    redone += 1;
                }
                assert_eq!(redone, undone, "tur {}", round);
                assert_eq!(d.script.entries, finalstate, "tur {}: yineleme son duruma dönmedi", round);
            }
        }
    }

    #[test]
    fn heavy_undo_is_pruned() {
        let mut d = doc();
        for i in 0..12 {
            let mut s = d.script.clone();
            // satır sayısını değiştir → yapısal değişiklik
            s.entries.push(Entry::Dialogue(Dialogue { text: format!("x{}", i), ..Default::default() }));
            d.apply_new_script("yapısal", s);
        }
        // küçük yapısal değişiklikler artık yalnızca farkı saklar: hepsi geri alınabilir
        let mut n = 0;
        while d.undo().is_some() { n += 1; }
        assert_eq!(n, 12);
        // büyük (>5000 girdi) yapısal değişiklikler en fazla 8 tane tutulur
        let mut d = doc();
        for i in 0..12 {
            let mut s = d.script.clone();
            for k in 0..6000 { s.entries.push(Entry::Dialogue(Dialogue { text: format!("x{i}-{k}"), ..Default::default() })); }
            d.apply_new_script("büyük", s);
        }
        let mut n = 0;
        while d.undo().is_some() { n += 1; }
        assert!(n <= 8, "{}", n);
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

    #[test]
    fn apply_new_script_stores_only_changed_range() {
        let ds: Vec<Dialogue> = (0..2000).map(|i| Dialogue { start_time: i * 1000, end_time: i * 1000 + 900, text: format!("satır {i}"), ..Default::default() }).collect();
        let mut doc = Document::new(Script::from_dialogues(ds));
        let orig = doc.script.entries.clone();
        let mut s = doc.script.clone();
        let at = s.entries.iter().position(|e| matches!(e, Entry::Dialogue(d) if d.text == "satır 1000")).unwrap();
        // bir satırı üç satırla değiştir (hareket izleme gibi)
        let Entry::Dialogue(d) = s.entries[at].clone() else { unreachable!() };
        let parts: Vec<Entry> = (0..3).map(|k| Entry::Dialogue(Dialogue { text: format!("parça {k}"), ..d.clone() })).collect();
        s.entries.splice(at..=at, parts);
        doc.apply_new_script("böl", s.clone());
        let t = doc.undo.last().unwrap();
        let stored: usize = t.changes.iter().map(|c| match c { Change::Insert { items, .. } | Change::Remove { items, .. } => items.len(), _ => 1 }).sum();
        assert_eq!(stored, 4, "yalnızca 1 silinen + 3 eklenen saklanmalı");
        assert_eq!(doc.script.entries, s.entries);
        doc.undo();
        assert_eq!(doc.script.entries, orig);
        doc.redo();
        assert_eq!(doc.script.entries, s.entries);
        // tekrarlayan içerikte önek/sonek çakışmaz
        let mut d2 = Document::new(Script::from_dialogues(vec![Dialogue::default(), Dialogue::default()]));
        let mut s2 = d2.script.clone();
        let last = s2.entries.last().cloned().unwrap();
        s2.entries.push(last);
        d2.apply_new_script("çoğalt", s2.clone());
        assert_eq!(d2.script.entries, s2.entries);
        d2.undo();
        assert_eq!(d2.script.entries.len(), s2.entries.len() - 1);
    }

    #[test]
    fn paste_over_fields() {
        let mut d = Document::new(Script::from_dialogues(vec![
            Dialogue { start_time: 0, end_time: 1000, text: "bir".into(), ..Default::default() },
            Dialogue { start_time: 1000, end_time: 2000, text: "iki".into(), ..Default::default() },
            Dialogue { start_time: 2000, end_time: 3000, text: "üç".into(), ..Default::default() },
        ]));
        let rows: Vec<usize> = d.script.entries.iter().enumerate().filter(|(_, e)| matches!(e, Entry::Dialogue(_))).map(|(i, _)| i).collect();
        let clip = "Dialogue: 3,0:00:10.00,0:00:11.00,Tabela,Kenji,0,0,0,,{\\an8}one\nDialogue: 4,0:00:20.00,0:00:21.00,Tabela,,0,0,0,,two\n";
        d.select(vec![rows[1], rows[2]], rows[1]);
        // yalnızca metin + stil
        let mut f = [false; 10];
        f[3] = true;
        f[9] = true;
        assert_eq!(d.paste_over(clip, &f), 2);
        let ds: Vec<&Dialogue> = d.script.dialogues().collect();
        assert_eq!((ds[0].text.as_str(), ds[1].text.as_str(), ds[2].text.as_str()), ("bir", "{\\an8}one", "two"));
        assert_eq!((ds[1].style.as_str(), ds[1].start_time, ds[1].layer), ("Tabela", 1000, 0));
        d.undo();
        assert_eq!(d.script.dialogues().nth(1).unwrap().text, "iki");
    }
}
