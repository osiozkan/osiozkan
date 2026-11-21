//! Lua betik dosyalarını izler: kaydedilince arayüze haber verir (betik geliştirirken
//! "Betikleri yeniden yükle"ye basmadan değişiklik görünür). Arka plan iş parçacığı saniyede bir
//! dosya zamanlarını okur; yalnızca değişiklikte arayüzü uyandırır (boştayken çizim yok).
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{channel, Receiver};
use std::sync::{Arc, Mutex};

pub struct ScriptWatch {
    paths: Arc<Mutex<Vec<PathBuf>>>,
    generation: Arc<AtomicU64>,
    pub rx: Receiver<()>,
}

fn is_script(p: &Path) -> bool {
    p.extension().map_or(false, |e| e.eq_ignore_ascii_case("lua") || e.eq_ignore_ascii_case("moon"))
}

/// Dosyaların (klasörse içindeki .lua/.moon'ların, bir alt düzey dahil) ad + zaman + boyut özeti.
pub fn fingerprint(paths: &[PathBuf]) -> u64 {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    fn file(h: &mut impl Hasher, p: &Path) {
        if let Ok(m) = std::fs::metadata(p) {
            p.hash(h);
            m.len().hash(h);
            if let Ok(t) = m.modified() { t.hash(h); }
        }
    }
    fn dir(h: &mut impl Hasher, d: &Path, depth: u32) {
        let Ok(rd) = std::fs::read_dir(d) else { return };
        let mut v: Vec<PathBuf> = rd.flatten().map(|e| e.path()).collect();
        v.sort();
        for p in v {
            if p.is_dir() {
                if depth > 0 { dir(h, &p, depth - 1) }
            } else if is_script(&p) {
                file(h, &p)
            }
        }
    }
    for p in paths {
        if p.is_dir() { dir(&mut h, p, 1) } else { file(&mut h, p) }
    }
    h.finish()
}

impl ScriptWatch {
    pub fn spawn(ctx: eframe::egui::Context) -> Self {
        let paths: Arc<Mutex<Vec<PathBuf>>> = Default::default();
        let generation = Arc::new(AtomicU64::new(0));
        let (tx, rx) = channel();
        let (p2, g2) = (paths.clone(), generation.clone());
        std::thread::Builder::new()
            .name("betik-izleyici".into())
            .spawn(move || {
                let mut last: Option<(u64, u64)> = None; // (kuşak, özet)
                loop {
                    std::thread::sleep(std::time::Duration::from_millis(800));
                    let g = g2.load(Ordering::Relaxed);
                    let list = p2.lock().map(|v| v.clone()).unwrap_or_default();
                    let f = fingerprint(&list);
                    if let Some((lg, lf)) = last {
                        // liste değiştiyse (betik eklendi) yeni taban, uyarı yok
                        if lg == g && lf != f {
                            if tx.send(()).is_err() { return; }
                            ctx.request_repaint();
                        }
                    }
                    last = Some((g, f));
                }
            })
            .ok();
        ScriptWatch { paths, generation, rx }
    }

    /// İzlenecek dosya ve klasörleri değiştirir.
    pub fn set(&self, v: Vec<PathBuf>) {
        if let Ok(mut p) = self.paths.lock() {
            if *p != v {
                *p = v;
                self.generation.fetch_add(1, Ordering::Relaxed);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ozet_degisince_farkli() {
        let d = std::env::temp_dir().join(format!("osisub-watch-{}", std::process::id()));
        std::fs::create_dir_all(d.join("alt")).unwrap();
        std::fs::write(d.join("a.lua"), "x").unwrap();
        std::fs::write(d.join("not.txt"), "x").unwrap();
        let a = fingerprint(&[d.clone()]);
        std::fs::write(d.join("not.txt"), "xyz").unwrap();
        assert_eq!(a, fingerprint(&[d.clone()]), "betik olmayan dosya sayılmaz");
        std::fs::write(d.join("alt/b.moon"), "y").unwrap();
        let b = fingerprint(&[d.clone()]);
        assert_ne!(a, b);
        std::fs::write(d.join("a.lua"), "xx").unwrap();
        assert_ne!(b, fingerprint(&[d.clone()]));
        let _ = std::fs::remove_dir_all(&d);
    }
}
