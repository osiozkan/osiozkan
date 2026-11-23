//! Aegisub'ın video karesine bağlı satır komutları (kaynaktaki davranışla birebir):
//! `edit/line/split/before|after`, `time/snap/scene`, `time/frame/current`,
//! `time/length|start/increase|decrease`, `time/lead/in|out`, `subtitle/select/visible`,
//! `subtitle/insert/before|after/videotime`, `grid/move/up|down`.
use crate::Document;
use osisub_core::ass::{Dialogue, Entry};
use osisub_core::vfr::Framerate;

impl Document {
    fn dlg(&self, i: usize) -> Option<&Dialogue> {
        match self.script.entries.get(i) {
            Some(Entry::Dialogue(d)) => Some(d),
            _ => None,
        }
    }

    /// Aegisub `edit/line/split/after` (`after`) / `split/before`: seçili her satır çoğaltılır, kopyalar
    /// bitişik seçim bloğunun hemen arkasına eklenir. Kare satırın içindeyse *after*: asıl satır bu karede
    /// biter, kopya sonraki karede başlar; *before*: asıl önceki karede biter, kopya bu karede başlar.
    /// Kare satırın dışındaysa kopya yalnızca o tek kareyi kaplar (kare kare tipografi).
    /// Yeni seçim kopyalardır, etkin satır ilk kopya. Dönüş: oluşan kopya sayısı.
    pub fn split_lines_at_frame(&mut self, fr: &Framerate, cur: i64, after: bool) -> usize {
        let sel = self.target_lines();
        if sel.is_empty() {
            return 0;
        }
        let in_sel = |i: usize| sel.binary_search(&i).is_ok();
        let mut out: Vec<Entry> = Vec::with_capacity(self.script.entries.len() + sel.len());
        let mut new_sel = vec![];
        let n = self.script.entries.len();
        let mut i = 0;
        while i < n {
            if !in_sel(i) {
                out.push(self.script.entries[i].clone());
                i += 1;
                continue;
            }
            let mut j = i;
            while j < n && in_sel(j) {
                j += 1;
            }
            let mut copies = vec![];
            for k in i..j {
                let Some(old) = self.dlg(k) else { continue };
                let (mut old, mut new) = (old.clone(), old.clone());
                let (s, e) = (fr.frame_at_start(old.start_time), fr.frame_at_end(old.end_time));
                if cur < s || cur > e {
                    new.start_time = fr.time_at_start(cur);
                    new.end_time = fr.time_at_end(cur);
                } else if after {
                    old.end_time = fr.time_at_end(cur);
                    new.start_time = fr.time_at_start(cur + 1);
                } else {
                    old.end_time = fr.time_at_end(cur - 1);
                    new.start_time = fr.time_at_start(cur);
                }
                out.push(Entry::Dialogue(old));
                copies.push(Entry::Dialogue(new));
            }
            for c in copies {
                new_sel.push(out.len());
                out.push(c);
            }
            i = j;
        }
        let mut s = self.script.clone();
        s.entries = out;
        self.apply_new_script(if after { "Satırları kareden sonra böl" } else { "Satırları kareden önce böl" }, s);
        let k = new_sel.len();
        if let Some(&a) = new_sel.first() {
            self.select(new_sel, a);
        }
        k
    }

    /// Aegisub `time/snap/scene`: seçili satırları `cur` karesini çevreleyen anahtar karelere ayarlar
    /// (başlangıç = önceki anahtar kare ya da kendisi, bitiş = sonraki anahtar kareden bir önceki kare;
    /// son anahtar kareden sonra videonun sonu). `kf`: anahtar kare numaraları (sıralı).
    pub fn snap_to_scene(&mut self, fr: &Framerate, kf: &[i64], cur: i64, frame_count: i64) -> bool {
        if kf.is_empty() {
            return false;
        }
        let (prev, next) = if cur < kf[0] {
            (0, kf[0])
        } else if cur >= *kf.last().unwrap() {
            (*kf.last().unwrap(), frame_count)
        } else {
            let p = kf.partition_point(|&k| k < cur);
            if kf[p] == cur { (kf[p], kf[p + 1]) } else { (kf[p - 1], kf[p]) }
        };
        let (a, b) = (fr.time_at_start(prev), fr.time_at_end(next - 1));
        let t = self.target_lines();
        self.edit("Sahneye yapıştır", |tx| {
            for &i in &t {
                tx.dialogue_mut(i, |d| {
                    d.start_time = a;
                    d.end_time = b;
                });
            }
        });
        true
    }

    /// Aegisub `time/frame/current`: seçimi, etkin satır `cur` karesinde başlayacak kadar kaydırır.
    pub fn shift_to_frame(&mut self, fr: &Framerate, cur: i64) -> bool {
        let Some(act) = self.dlg(self.active) else { return false };
        let by = fr.time_at_start(cur).max(0) - act.start_time;
        let mut t = self.target_lines();
        if !t.contains(&self.active) {
            t.push(self.active);
        }
        self.edit("Geçerli kareye kaydır", |tx| {
            for &i in &t {
                tx.dialogue_mut(i, |d| {
                    d.start_time = (d.start_time + by).max(0);
                    d.end_time = (d.end_time + by).max(0);
                });
            }
        });
        true
    }

    /// Aegisub ses zamanlama tuşları: `end` ise bitişi (`time/length/*`), değilse başlangıcı
    /// (`time/start/*`) `delta_ms` kaydırır; bitiş başlangıçtan önceye, başlangıç bitişten sonraya geçmez.
    pub fn nudge_active(&mut self, end: bool, delta_ms: i64) -> bool {
        let at = self.active;
        let Some(d) = self.dlg(at) else { return false };
        let (s, e) = (d.start_time, d.end_time);
        let (ns, ne) = if end { (s, (e + delta_ms).max(s)) } else { ((s + delta_ms).min(e).max(0), e) };
        if (ns, ne) == (s, e) {
            return false;
        }
        let key = format!("time:{at}");
        self.edit_keyed("Zamanı kaydır", Some(&key), |tx| {
            tx.dialogue_mut(at, |d| {
                d.start_time = ns;
                d.end_time = ne;
            })
        });
        true
    }

    /// Aegisub `time/lead/in|out`: etkin satıra giriş/çıkış payı ekler.
    pub fn add_lead(&mut self, lead_in: i64, lead_out: i64) -> bool {
        let at = self.active;
        let Some(d) = self.dlg(at) else { return false };
        let (s, e) = ((d.start_time - lead_in).max(0), d.end_time + lead_out);
        self.edit("Giriş/çıkış payı ekle", |tx| {
            tx.dialogue_mut(at, |d| {
                d.start_time = s;
                d.end_time = e;
            })
        });
        true
    }

    /// Aegisub `subtitle/select/visible`: `cur` karesinde görünen (yorum dahil) tüm satırları seçer;
    /// ilki etkin olur. Dönüş: seçilen sayısı (hiçbiri yoksa seçim boşalır, etkin satır değişmez).
    pub fn select_visible(&mut self, fr: &Framerate, cur: i64) -> usize {
        let v: Vec<usize> = (0..self.script.entries.len())
            .filter(|&i| self.dlg(i).map_or(false, |d| fr.frame_at_start(d.start_time) <= cur && fr.frame_at_end(d.end_time) >= cur))
            .collect();
        let n = v.len();
        let a = v.first().copied().unwrap_or(self.active);
        self.select(v, a);
        n
    }

    /// Aegisub `subtitle/insert/before|after/videotime`: etkin satırın önüne/arkasına `cur` karesinde
    /// başlayan, `dur_ms` süren, etkin satırın stilinde boş satır ekler.
    pub fn insert_at_frame(&mut self, fr: &Framerate, cur: i64, after: bool, dur_ms: i64) -> Option<usize> {
        let style = self.dlg(self.active)?.style.clone();
        let st = fr.time_at_start(cur);
        let pos = if after { self.active + 1 } else { self.active };
        let d = Dialogue { start_time: st, end_time: st + dur_ms, style, ..Default::default() };
        self.edit("Satır ekle", |tx| tx.insert(pos, vec![Entry::Dialogue(d)]));
        self.select(vec![pos], pos);
        Some(pos)
    }

    /// Aegisub `grid/move/up|down`: seçili satırları (bloklar halinde) bir satır yukarı/aşağı taşır;
    /// en üstte/altta olan blok yerinde kalır. Diyalog olmayan girdilerle (stil vb.) yer değişmez.
    pub fn move_selected(&mut self, up: bool) -> bool {
        let t = self.target_lines();
        if t.is_empty() {
            return false;
        }
        let mut e = self.script.entries.clone();
        let is_d = |e: &Entry| matches!(e, Entry::Dialogue(_));
        let mut sel: Vec<usize> = t.clone();
        let mut moved = false;
        let order: Vec<usize> = if up { (0..sel.len()).collect() } else { (0..sel.len()).rev().collect() };
        for k in order {
            let i = sel[k];
            let j = if up { i.checked_sub(1) } else { Some(i + 1) };
            let Some(j) = j.filter(|&j| j < e.len() && is_d(&e[j]) && !sel.contains(&j)) else { continue };
            e.swap(i, j);
            sel[k] = j;
            moved = true;
        }
        if !moved {
            return false;
        }
        let act_k = t.iter().position(|&i| i == self.active);
        let mut s = self.script.clone();
        s.entries = e;
        self.apply_new_script(if up { "Satırları yukarı taşı" } else { "Satırları aşağı taşı" }, s);
        let a = act_k.map(|k| sel[k]).unwrap_or(sel[0]);
        self.select(sel, a);
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use osisub_core::ass::Script;

    fn doc(times: &[(i64, i64)]) -> (Document, usize) {
        let ds = times.iter().enumerate().map(|(k, &(a, b))| Dialogue { start_time: a, end_time: b, text: format!("s{k}"), ..Default::default() }).collect();
        let d = Document::new(Script::from_dialogues(ds));
        let f = d.script.entries.iter().position(|e| matches!(e, Entry::Dialogue(_))).unwrap();
        (d, f)
    }
    fn times(d: &Document) -> Vec<(i64, i64, String)> {
        d.script.dialogues().map(|x| (x.start_time, x.end_time, x.text.clone())).collect()
    }

    #[test]
    fn split_after_and_before_like_aegisub() {
        let fr = Framerate::cfr(25.0); // kare 40 ms
        let (mut d, f) = doc(&[(0, 2000), (2000, 4000), (5000, 6000)]);
        d.select(vec![f, f + 1], f);
        assert_eq!(d.split_lines_at_frame(&fr, 25, true), 2); // kare 25 = 1000 ms
        // bitişik blok: asıllar sırada, kopyalar bloğun arkasında
        let t = times(&d);
        assert_eq!(t[0], (0, fr.time_at_end(25), "s0".into()));
        assert_eq!(fr.frame_at_end(t[0].1), 25);
        assert_eq!(t[1].2, "s1");
        assert_eq!((fr.frame_at_start(t[2].0), t[2].1, t[2].2.as_str()), (26, 2000, "s0"));
        // s1 karenin dışında: kopya tek kare
        let one = &t[3];
        assert_eq!((fr.frame_at_start(one.0), fr.frame_at_end(one.1), one.2.as_str()), (25, 25, "s1"));
        assert_eq!(d.selection, vec![f + 2, f + 3]);
        assert_eq!(d.active, f + 2);

        let (mut d, f) = doc(&[(0, 2000)]);
        d.select(vec![f], f);
        d.split_lines_at_frame(&fr, 25, false);
        let t = times(&d);
        assert_eq!(fr.frame_at_end(t[0].1), 24);
        assert_eq!(fr.frame_at_start(t[1].0), 25);
    }

    #[test]
    fn snap_scene_and_shift_to_frame() {
        let fr = Framerate::cfr(25.0);
        let (mut d, f) = doc(&[(0, 100), (300, 400)]);
        d.select(vec![f, f + 1], f);
        let kf = [0, 50, 100];
        d.snap_to_scene(&fr, &kf, 60, 200);
        for (a, b, _) in times(&d) {
            assert_eq!((fr.frame_at_start(a), fr.frame_at_end(b)), (50, 99));
        }
        d.snap_to_scene(&fr, &kf, 100, 200); // tam anahtar karede → o kare ile sonraki arası
        assert_eq!(fr.frame_at_end(times(&d)[0].1), 199);
        d.snap_to_scene(&fr, &[10, 20], 5, 200); // ilk anahtar kareden önce
        assert_eq!((fr.frame_at_start(times(&d)[0].0), fr.frame_at_end(times(&d)[0].1)), (0, 9));

        let (mut d, f) = doc(&[(1000, 2000), (3000, 3500)]);
        d.select(vec![f, f + 1], f + 1);
        d.shift_to_frame(&fr, 100); // etkin satır 100. karede başlar; diğeri aynı farkla
        let by = fr.time_at_start(100) - 3000;
        assert_eq!(times(&d).iter().map(|t| (t.0, t.1)).collect::<Vec<_>>(), vec![(1000 + by, 2000 + by), (3000 + by, 3500 + by)]);
        assert_eq!(fr.frame_at_start(times(&d)[1].0), 100);
    }

    #[test]
    fn nudge_lead_visible_insert_move() {
        let fr = Framerate::cfr(25.0);
        let (mut d, f) = doc(&[(1000, 1010), (500, 3000), (5000, 6000)]);
        d.select(vec![f], f);
        assert!(!d.nudge_active(true, -10) || times(&d)[0].1 >= times(&d)[0].0);
        assert_eq!(times(&d)[0].1, 1000); // başlangıçtan önceye geçmez
        d.nudge_active(false, 10);
        assert_eq!(times(&d)[0].0, 1000);
        d.nudge_active(false, -10);
        d.nudge_active(false, -10);
        assert_eq!(times(&d)[0].0, 980);
        d.add_lead(200, 300);
        assert_eq!((times(&d)[0].0, times(&d)[0].1), (780, 1300));

        assert_eq!(d.select_visible(&fr, 30), 2); // 1200 ms: s0 (780-1300) ve s1
        assert_eq!(d.selection, vec![f, f + 1]);
        assert_eq!(d.active, f);

        d.select(vec![f + 1], f + 1);
        let p = d.insert_at_frame(&fr, 100, true, 2000).unwrap();
        assert_eq!(p, f + 2);
        let st = fr.time_at_start(100);
        assert_eq!((times(&d)[2].0, times(&d)[2].1), (st, st + 2000));

        // taşı: s2 (f+3) yukarı → s1'in üstüne değil, yeni satırla yer değiştirir
        d.select(vec![f + 3], f + 3);
        assert!(d.move_selected(true));
        assert_eq!(times(&d)[2].2, "s2");
        assert_eq!(d.selection, vec![f + 2]);
        // en üstteki satır yukarı gidemez (öncesinde stil girdileri var)
        d.select(vec![f], f);
        assert!(!d.move_selected(true));
        d.select(vec![f, f + 1], f);
        assert!(d.move_selected(false));
        assert_eq!(times(&d).iter().map(|t| t.2.as_str()).take(3).collect::<Vec<_>>(), vec!["s2", "s0", "s1"]);
        assert_eq!(d.selection, vec![f + 1, f + 2]);
    }
}
