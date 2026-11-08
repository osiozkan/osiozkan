use kitsune_core::{self as core, tools};
use kitsune_lua::{HostOptions, Runner};
use std::collections::HashMap;

fn usage() -> ! {
    eprintln!("kitsune <command>\n\
  convert <in> <out>                    ASS/SRT/VTT dönüştür\n\
  check <file> [--cps N] [--len N]      kalite kontrol (CPS, çakışma, satır uzunluğu...)\n\
  shift <in> <out> <ms>                 tüm zamanlamaları kaydır\n\
  fix <in> <out>                        çakışma/süre/ardışık-aynı düzeltmeleri\n\
  macros <script.lua>                   Lua betiğindeki makroları listele\n\
  run <script.lua> <in> <out> [--macro N|NAME] [--sel a,b,c] [--set key=val]... [--fps F]\n\
                                         Aegisub Automation 4 makrosu çalıştır");
    std::process::exit(2)
}

fn main() {
    let a: Vec<String> = std::env::args().skip(1).collect();
    if a.is_empty() { usage(); }
    let r: Result<(), String> = (|| match a[0].as_str() {
        "convert" if a.len() >= 3 => { let s = core::load(&a[1])?; core::save(&s, &a[2]) }
        "check" if a.len() >= 2 => {
            let s = core::load(&a[1])?;
            let mut cfg = tools::QcConfig::default();
            let mut i = 2;
            while i + 1 < a.len() {
                match a[i].as_str() { "--cps" => cfg.max_cps = a[i + 1].parse().map_err(|_| "bad --cps")?, "--len" => cfg.max_line_len = a[i + 1].parse().map_err(|_| "bad --len")?, _ => {} }
                i += 2;
            }
            let issues = tools::check(&s, &cfg);
            for it in &issues { println!("#{:<5} {:<18} {}", it.line, it.kind, it.msg); }
            println!("{} sorun", issues.len());
            Ok(())
        }
        "shift" if a.len() >= 4 => { let mut s = core::load(&a[1])?; tools::shift(&mut s, a[3].parse().map_err(|_| "bad ms")?, None); core::save(&s, &a[2]) }
        "fix" if a.len() >= 3 => {
            let mut s = core::load(&a[1])?;
            let o = tools::fix_overlaps(&mut s); let m = tools::merge_duplicates(&mut s); let d = tools::fix_short_durations(&mut s, 500, 20.0);
            eprintln!("çakışma: {o}, birleştirilen: {m}, uzatılan: {d}");
            core::save(&s, &a[2])
        }
        "macros" if a.len() >= 2 => {
            let mut r = Runner::new(core::Script::new_default(), HostOptions::default()).map_err(|e| e.to_string())?;
            r.load_file(&a[1]).map_err(|e| e.to_string())?;
            for (i, m) in r.macros.iter().enumerate() { println!("{}\t{}\t{}", i, m.name, m.description); }
            Ok(())
        }
        "run" if a.len() >= 4 => {
            let s = core::load(&a[2])?;
            let mut opts = HostOptions::default();
            let (mut which, mut sel): (Option<String>, Option<Vec<usize>>) = (None, None);
            let mut i = 4;
            while i < a.len() {
                match a[i].as_str() {
                    "--macro" => { which = a.get(i + 1).cloned(); i += 2 }
                    "--sel" => { sel = a.get(i + 1).map(|v| v.split(',').filter_map(|x| x.parse().ok()).collect()); i += 2 }
                    "--fps" => { opts.fps = a.get(i + 1).and_then(|v| v.parse().ok()); i += 2 }
                    "--set" => { if let Some((k, v)) = a.get(i + 1).and_then(|kv| kv.split_once('=')) { opts.dialog_overrides.insert(k.into(), v.into()); } i += 2 }
                    _ => i += 1,
                }
            }
            let _: &HashMap<String, String> = &opts.dialog_overrides;
            let mut r = Runner::new(s.clone(), opts).map_err(|e| e.to_string())?;
            r.load_file(&a[1]).map_err(|e| e.to_string())?;
            let idx = match which {
                None => 0,
                Some(w) => w.parse::<usize>().ok().or_else(|| r.macros.iter().position(|m| m.name == w)).ok_or("makro bulunamadı")?,
            };
            let first_ev = s.entries.iter().position(|e| matches!(e, core::Entry::Event(_))).map(|p| p + 1).unwrap_or(1);
            let sel = sel.unwrap_or_else(|| (first_ev..=s.entries.len()).collect());
            let active = sel.first().copied().unwrap_or(1);
            let res = r.run(idx, &sel, active)?;
            for l in &res.log { eprintln!("{l}"); }
            if res.cancelled { eprintln!("makro iptal edildi"); return Ok(()); }
            core::save(&r.script(), &a[3])
        }
        _ => usage(),
    })();
    if let Err(e) = r { eprintln!("hata: {e}"); std::process::exit(1); }
}
