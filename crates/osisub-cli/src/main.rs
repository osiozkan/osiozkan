//! osisub: komut satırı arayüzü.
use osisub_core::ass::{Entry, Script};
use osisub_core::qc::{self, Rules};
use osisub_lua::{Engine, HeadlessHost};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

const HELP: &str = "osisub - anime altyazı aracı (komut satırı)

KULLANIM:
  osisub convert <girdi> <çıktı>          .ass/.ssa/.srt/.vtt arasında dönüştür
  osisub check <dosya> [--cps N] [--len N] [--min-ms N] [--max-ms N] [--gap N]
  osisub fix <girdi> [çıktı] [--overlaps] [--dupes] [--empty] [--clean] [--sort] [--fps N --snap]
  osisub shift <girdi> <ms> [çıktı]       tüm satırları kaydır (negatif olabilir)
  osisub macros <betik.lua>...            betiklerdeki makroları listele
  osisub run <girdi> <betik.lua> <makro-no|ad> [çıktı] [--param ad=değer]... [--select 1,2,3] [--fps N]
  osisub info <dosya>                     özet bilgi
";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match real_main(args) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("hata: {}", e);
            ExitCode::FAILURE
        }
    }
}

fn flag(args: &mut Vec<String>, name: &str) -> bool {
    if let Some(i) = args.iter().position(|a| a == name) {
        args.remove(i);
        true
    } else {
        false
    }
}

fn opt(args: &mut Vec<String>, name: &str) -> Option<String> {
    let i = args.iter().position(|a| a == name)?;
    if i + 1 < args.len() {
        let v = args.remove(i + 1);
        args.remove(i);
        Some(v)
    } else {
        None
    }
}

fn load(path: &str) -> Result<Script, String> {
    osisub_core::io::load(Path::new(path))
}

fn save(s: &Script, path: &str) -> Result<(), String> {
    osisub_core::io::save(s, Path::new(path))
}

fn real_main(mut args: Vec<String>) -> Result<(), String> {
    if args.is_empty() || args[0] == "-h" || args[0] == "--help" {
        print!("{}", HELP);
        return Ok(());
    }
    let cmd = args.remove(0);
    match cmd.as_str() {
        "convert" => {
            let [i, o] = args.as_slice() else { return Err("kullanım: convert <girdi> <çıktı>".into()) };
            save(&load(i)?, o)
        }
        "info" => {
            let s = load(args.first().ok_or("dosya gerekli")?)?;
            let d = s.dialogues().count();
            let (w, h) = s.res();
            println!("çözünürlük: {}x{}\nstil: {}\ndiyalog: {} (yorum: {})", w, h, s.styles().count(), d, s.dialogues().filter(|x| x.comment).count());
            if let Some(last) = s.dialogues().map(|d| d.end_time).max() {
                println!("süre: {}", osisub_core::time::format_ass(last));
            }
            Ok(())
        }
        "check" => {
            let mut r = Rules::default();
            if let Some(v) = opt(&mut args, "--cps") { r.max_cps = v.parse().map_err(|_| "--cps sayı olmalı")?; }
            if let Some(v) = opt(&mut args, "--len") { r.max_line_len = v.parse().map_err(|_| "--len sayı olmalı")?; }
            if let Some(v) = opt(&mut args, "--min-ms") { r.min_duration_ms = v.parse().map_err(|_| "--min-ms sayı olmalı")?; }
            if let Some(v) = opt(&mut args, "--max-ms") { r.max_duration_ms = v.parse().map_err(|_| "--max-ms sayı olmalı")?; }
            if let Some(v) = opt(&mut args, "--gap") { r.min_gap_ms = v.parse().map_err(|_| "--gap sayı olmalı")?; }
            let s = load(args.first().ok_or("dosya gerekli")?)?;
            let issues = qc::check(&s, &r);
            for is in &issues {
                if let Entry::Dialogue(d) = &s.entries[is.entry] {
                    println!("{} [{}] {:?}: {}", is.entry + 1, osisub_core::time::format_ass(d.start_time), is.kind, is.message);
                }
            }
            println!("{} sorun", issues.len());
            if issues.is_empty() { Ok(()) } else { Err(format!("{} sorun bulundu", issues.len())) }
        }
        "fix" => {
            let all = !["--overlaps", "--dupes", "--empty", "--clean", "--sort", "--snap"].iter().any(|f| args.contains(&f.to_string()));
            let (ov, du, em, cl, so, sn) = (flag(&mut args, "--overlaps"), flag(&mut args, "--dupes"), flag(&mut args, "--empty"), flag(&mut args, "--clean"), flag(&mut args, "--sort"), flag(&mut args, "--snap"));
            let fps: Option<f64> = opt(&mut args, "--fps").and_then(|v| v.parse().ok());
            let inp = args.first().ok_or("girdi gerekli")?.clone();
            let out = args.get(1).cloned().unwrap_or_else(|| inp.clone());
            let mut s = load(&inp)?;
            if all || so { qc::sort_by_start(&mut s); }
            if all || du { println!("yinelenen silindi: {}", qc::remove_duplicates(&mut s)); }
            if all || em { println!("boş silindi: {}", qc::remove_empty(&mut s)); }
            if all || cl { for d in s.dialogues_mut() { d.text = qc::clean_text(&d.text); } }
            if sn { qc::snap_to_frames(&mut s, fps.ok_or("--snap için --fps gerekli")?); }
            if all || ov { println!("çakışma düzeltildi: {}", qc::fix_overlaps(&mut s)); }
            save(&s, &out)
        }
        "shift" => {
            let ms: i64 = args.get(1).ok_or("ms gerekli")?.parse().map_err(|_| "ms sayı olmalı")?;
            let inp = args[0].clone();
            let out = args.get(2).cloned().unwrap_or_else(|| inp.clone());
            let mut s = load(&inp)?;
            qc::shift(&mut s, ms, None);
            save(&s, &out)
        }
        "macros" => {
            let mut e = new_engine(HeadlessHost::default())?;
            for p in &args {
                let info = e.load_script(Path::new(p)).map_err(|e| e.to_string())?;
                println!("{} — {} ({})", info.name, info.description, info.file);
            }
            for m in e.macros() {
                println!("  [{}] {} — {} ({:?})", m.id, m.name, m.description, m.kind);
            }
            Ok(())
        }
        "run" => {
            let mut params = HashMap::new();
            while let Some(p) = opt(&mut args, "--param") {
                if let Some((k, v)) = p.split_once('=') { params.insert(k.to_string(), v.to_string()); }
            }
            let sel_arg = opt(&mut args, "--select");
            let fps = opt(&mut args, "--fps").and_then(|v| v.parse().ok());
            if args.len() < 3 { return Err("kullanım: run <girdi> <betik.lua> <makro> [çıktı]".into()); }
            let (inp, lua, mac) = (args[0].clone(), args[1].clone(), args[2].clone());
            let out = args.get(3).cloned().unwrap_or_else(|| inp.clone());
            let mut e = new_engine(HeadlessHost { params, fps, ..Default::default() })?;
            let script = load(&inp)?;
            let n = script.entries.len();
            e.set_script(script);
            if let Some(dir) = Path::new(&inp).canonicalize().ok().and_then(|p| p.parent().map(|x| x.to_path_buf())) {
                e.set_property("script_dir", &dir.to_string_lossy());
            }
            e.set_property("script_file", &inp);
            e.load_script(Path::new(&lua)).map_err(|e| e.to_string())?;
            let id = match mac.parse::<usize>() {
                Ok(i) => i,
                Err(_) => e.macros().iter().find(|m| m.name == mac).map(|m| m.id).ok_or(format!("makro bulunamadı: {}", mac))?,
            };
            let sel: Vec<usize> = match sel_arg {
                Some(s) => s.split(',').filter_map(|x| x.trim().parse().ok()).collect(),
                None => (1..=n).filter(|i| matches!(e.script().entries[i - 1], Entry::Dialogue(_))).collect(),
            };
            let active = sel.first().copied().unwrap_or(0);
            if !e.validate_macro(id, &sel, active) {
                return Err("makro bu seçim için geçerli değil".into());
            }
            let r = e.run_macro(id, &sel, active).map_err(|e| e.to_string())?;
            if r.cancelled {
                eprintln!("makro iptal etti; çıktı yazılmadı");
                return Ok(());
            }
            save(&e.take_script(), &out)
        }
        _ => Err(format!("bilinmeyen komut: {}\n\n{}", cmd, HELP)),
    }
}

fn new_engine(host: HeadlessHost) -> Result<Engine, String> {
    let mut e = Engine::new(Box::new(host)).map_err(|e| e.to_string())?;
    if let Ok(exe) = std::env::current_exe() {
        if let Some(d) = exe.parent() {
            e.add_include_dir(PathBuf::from(d).join("include"));
        }
    }
    Ok(e)
}
