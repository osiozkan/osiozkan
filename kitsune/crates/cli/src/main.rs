use kitsune_core::{check, load_file, save_file, time};
use kitsune_lua::Engine;
use std::path::Path;
use std::process::exit;

const HELP: &str = "kitsune — anime altyazı aracı (komut satırı)

KULLANIM:
  kitsune info <dosya>                       özet bilgi
  kitsune convert <girdi> <çıktı>            ass/srt/vtt dönüştür (uzantıya göre)
  kitsune check <dosya>                      kalite denetimi (CPS, çakışma, satır uzunluğu…)
  kitsune shift <girdi> <çıktı> <±ms>        tüm zamanları kaydır
  kitsune lua list <betik.lua>               betiğin kayıtlı makroları
  kitsune lua run <betik.lua> <no> <girdi> <çıktı> [sel,sel,..]   makroyu çalıştır (no: 0'dan)";

fn die(m: &str) -> ! {
    eprintln!("hata: {}", m);
    exit(1)
}

fn main() {
    let a: Vec<String> = std::env::args().skip(1).collect();
    let s: Vec<&str> = a.iter().map(|x| x.as_str()).collect();
    match s.as_slice() {
        ["info", f] => {
            let d = load_file(Path::new(f)).unwrap_or_else(|e| die(&e));
            let (w, h) = d.play_res();
            println!("Başlık: {}\nÇözünürlük: {}x{}\nStil: {}\nSatır: {} ({} yorum)", d.info_get("Title").unwrap_or("-"), w, h, d.styles.len(), d.events.len(), d.events.iter().filter(|e| e.is_comment()).count());
            for w in &d.warnings { println!("uyarı: {}", w); }
        }
        ["convert", i, o] => {
            let d = load_file(Path::new(i)).unwrap_or_else(|e| die(&e));
            save_file(&d, Path::new(o)).unwrap_or_else(|e| die(&e));
        }
        ["check", f] => {
            let d = load_file(Path::new(f)).unwrap_or_else(|e| die(&e));
            let issues = check::run(&d, &check::Rules::default());
            for i in &issues {
                let e = &d.events[i.index];
                println!("#{} [{}] {:?}: {}", i.index + 1, time::format_ass(e.start), i.kind, i.message);
            }
            println!("{} sorun", issues.len());
            if !issues.is_empty() { exit(2) }
        }
        ["shift", i, o, ms] => {
            let ms: i64 = ms.parse().unwrap_or_else(|_| die("ms sayı olmalı"));
            let mut d = load_file(Path::new(i)).unwrap_or_else(|e| die(&e));
            for e in &mut d.events { e.start = (e.start + ms).max(0); e.end = (e.end + ms).max(0); }
            save_file(&d, Path::new(o)).unwrap_or_else(|e| die(&e));
        }
        ["lua", "list", script] => {
            let e = Engine::new().unwrap_or_else(|e| die(&e));
            e.load_script(Path::new(script)).unwrap_or_else(|er| die(&er));
            for (i, m) in e.macros().iter().enumerate() { println!("{}\t{}\t{}", i, m.name, m.description); }
        }
        ["lua", "run", script, no, i, o, rest @ ..] => {
            let e = Engine::new().unwrap_or_else(|e| die(&e));
            e.load_script(Path::new(script)).unwrap_or_else(|er| die(&er));
            let mut d = load_file(Path::new(i)).unwrap_or_else(|e| die(&e));
            let sel: Vec<usize> = match rest.first() {
                Some(r) => r.split(',').filter_map(|x| x.trim().parse::<usize>().ok()).collect(),
                None => (0..d.events.len()).collect(),
            };
            let no: usize = no.parse().unwrap_or_else(|_| die("makro no sayı olmalı"));
            let out = e.run(no, &mut d, &sel, sel.first().copied()).unwrap_or_else(|er| die(&er));
            for l in &out.logs { println!("{}", l); }
            if out.cancelled { eprintln!("makro iptal etti; dosya yazılmadı"); exit(3) }
            save_file(&d, Path::new(o)).unwrap_or_else(|e| die(&e));
        }
        _ => println!("{}", HELP),
    }
}
