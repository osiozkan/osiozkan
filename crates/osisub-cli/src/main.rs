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
  osisub check <dosya> [--cps N] [--len N] [--min-ms N] [--max-ms N] [--gap N] [--video v.mkv]
                (--video: sahne değişimine ≤3 kare yakın başlangıç/bitiş, 1-2 karelik titreme)
  osisub compare <eski> <yeni>     (eklenen/silinen/değişen satırlar)
  osisub to-drawing <girdi> --lines 5,6 [çıktı]   satırları aynı görünümde \\p1 çizimine çevir
  osisub perspective <dörtgen: x,y x,y x,y x,y> --size G,Y [--playres-y N --layoutres-y N]
                          w×h kutuyu 4 köşeye oturtan \\frx\\fry\\frz\\fscx\\fscy\\fax\\org etiketleri
  osisub shape <metin> [--font Ad] [--size N] [--bold] [--italic] [--fscx N] [--fscy N] [--fsp N]
                          metni ASS çizimine (\\p1) çevir; (0,0) = hücrenin sol üstü
  osisub merge <ana.ass> <ek.ass>... -o <çıktı> [--offset ms] [--sort] [--no-comments]
                          diyalog + tabela + OP/ED birleştir (stil çakışması, çözünürlük, yazı tipleri)
  osisub fix <girdi> [çıktı] [--overlaps] [--dupes] [--empty] [--clean] [--sort] [--fps N|--timecodes tc.txt --snap]
                          [--merge-same BOŞLUK_MS] [--merge-short KAR] [--recalc KAR/SN]
  osisub shift <girdi> <ms> [çıktı]       tüm satırları kaydır (negatif olabilir)
  osisub macros <betik.lua>...            betiklerdeki makroları listele
  osisub run <girdi> <betik.lua> <makro-no|ad> [çıktı] [--param ad=değer]... [--select 1,2,3] [--fps N] [--timecodes tc.txt]
  osisub info <dosya>                     özet bilgi
  osisub motion <girdi> <iz.txt> --lines 5,6 [çıktı] [--start-frame N] [--fps N|--timecodes tc.txt] [--no-pos] [--no-scale] [--no-rot] [--no-clip]
                          Mocha/AE keyframe verisini satırlara kare kare uygula (satır no: diyalog sırası, 1'den)
  osisub fixes <girdi> [çıktı] [--ellipsis] [--sentence] [--hi] [--min-dur N] [--min-gap N] [--tags] [--notes]
                          (--tags: etiketleri onar, --notes: {notları} sil)
  osisub resample <girdi> <genişlik> <yükseklik> [çıktı]
  osisub post <girdi> [çıktı] [--lead-in N] [--lead-out N] [--gap N] [--video dosya]
  osisub find <girdi> <desen> [--regex] [--case] [--replace METİN [çıktı]]
  osisub fonts <girdi>                    kullanılan yazı tipleri ve kurulu mu
  osisub batch <klasör> <uzantı> [--recursive]   klasördeki altyazıları toplu dönüştür

Lua include klasörleri: <yapılandırma>/osisub/automation/include, kurulu Aegisub'ın automation/include'u,
programın yanındaki include/ ve OSISUB_INCLUDE (yol listesi).

Desteklenen biçimler: ass, ssa, srt, vtt, lrc, sub (MicroDVD / SubViewer), sbv, smi (SAMI), ttml/dfxp; yazma: txt, csv.
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
    if args.first().map(|a| a == "--version" || a == "-V").unwrap_or(false) {
        println!("osisub {}", env!("CARGO_PKG_VERSION"));
        return Ok(());
    }
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
        "motion" => {
            let lines_arg = opt(&mut args, "--lines").ok_or("--lines gerekli (ör. --lines 5,6)")?;
            let start = opt(&mut args, "--start-frame").map(|v| v.parse::<i64>().map_err(|_| "--start-frame sayı olmalı")).transpose()?;
            let fps: Option<f64> = opt(&mut args, "--fps").and_then(|v| v.parse().ok());
            let tc = load_timecodes(&mut args)?;
            let mut o = osisub_core::motion::MotionOptions::default();
            o.position = !flag(&mut args, "--no-pos");
            o.scale = !flag(&mut args, "--no-scale");
            o.rotation = !flag(&mut args, "--no-rot");
            o.clip = !flag(&mut args, "--no-clip");
            if args.len() < 2 { return Err("kullanım: motion <girdi> <iz.txt> --lines 5,6 [çıktı]".into()); }
            let out = args.get(2).cloned().unwrap_or_else(|| args[0].clone());
            let mut s = load(&args[0])?;
            let data = osisub_core::motion::parse_ae(&std::fs::read_to_string(&args[1]).map_err(|e| format!("{}: {}", args[1], e))?)?;
            let fr = tc.unwrap_or_else(|| osisub_core::vfr::Framerate::cfr(fps.unwrap_or(data.fps)));
            let dl: Vec<usize> = s.entries.iter().enumerate().filter(|(_, e)| matches!(e, Entry::Dialogue(_))).map(|(i, _)| i).collect();
            let mut targets: Vec<usize> = vec![];
            for n in lines_arg.split(',') {
                let k: usize = n.trim().parse().map_err(|_| format!("geçersiz satır no: {n}"))?;
                targets.push(*dl.get(k.wrapping_sub(1)).ok_or(format!("satır yok: {k}"))?);
            }
            targets.sort_unstable();
            targets.dedup();
            let lines: Vec<_> = targets.iter().filter_map(|&i| match &s.entries[i] { Entry::Dialogue(d) => Some(d.clone()), _ => None }).collect();
            o.start_frame = start.unwrap_or_else(|| lines.iter().map(|d| fr.frame_at_start(d.start_time)).min().unwrap_or(0));
            let new = osisub_core::motion::apply(&s, &lines, &data, &fr, &o)?;
            let at = targets[0];
            for &i in targets.iter().rev() { s.entries.remove(i); }
            println!("{} satır → {} kare satırı (iz {} kare, ilk kare {})", lines.len(), new.len(), data.frames.len(), o.start_frame);
            s.entries.splice(at..at, new.into_iter().map(Entry::Dialogue));
            save(&s, &out)
        }
        "to-drawing" => {
            // satırları aynı görünümde \p1 çizimine çevir
            let lines_arg = opt(&mut args, "--lines").ok_or("--lines gerekli (ör. --lines 5,6)")?;
            if args.is_empty() { return Err("kullanım: to-drawing <girdi> --lines 5,6 [çıktı]".into()); }
            let out = args.get(1).cloned().unwrap_or_else(|| args[0].clone());
            let mut s = load(&args[0])?;
            let dl: Vec<usize> = s.entries.iter().enumerate().filter(|(_, e)| matches!(e, Entry::Dialogue(_))).map(|(i, _)| i).collect();
            let mut db = osisub_lua::fonts::FontDb::default();
            for n in lines_arg.split(',') {
                let k: usize = n.trim().parse().map_err(|_| format!("geçersiz satır no: {n}"))?;
                let i = *dl.get(k.wrapping_sub(1)).ok_or(format!("satır yok: {k}"))?;
                let Entry::Dialogue(d) = &s.entries[i] else { continue };
                let t = osisub_lua::fonts::line_to_drawing(&mut db, &s, d).map_err(|e| format!("satır {k}: {e}"))?;
                if let Entry::Dialogue(d) = &mut s.entries[i] { d.text = t; }
            }
            save(&s, &out)
        }
        "perspective" => {
            // perspective "x0,y0 x1,y1 x2,y2 x3,y3" --size G,Y  → dörtgene oturan etiketler
            let size = opt(&mut args, "--size").ok_or("--size genişlik,yükseklik gerekli")?;
            // kamera uzaklığı: 312.5 × PlayResY / LayoutResY (varsayılan ikisi eşit)
            let dist = match (opt(&mut args, "--playres-y"), opt(&mut args, "--layoutres-y")) {
                (Some(p), Some(l)) => osisub_core::perspective::dist_for(p.parse().map_err(|_| "--playres-y")?, l.parse().map_err(|_| "--layoutres-y")?),
                _ => osisub_core::perspective::DIST_PX,
            };
            let (w, h) = size.split_once(',').and_then(|(a, b)| Some((a.trim().parse::<f64>().ok()?, b.trim().parse::<f64>().ok()?))).ok_or("--size G,Y")?;
            let pts: Vec<(f64, f64)> = args.first().ok_or("dörtgen gerekli")?.split_whitespace().filter_map(|p| { let (a, b) = p.split_once(',')?; Some((a.parse().ok()?, b.parse().ok()?)) }).collect();
            if pts.len() != 4 { return Err("4 nokta gerekli: \"x,y x,y x,y x,y\"".into()); }
            let q = osisub_core::perspective::quad_from_clip(&format!("m {} {} l {} {} {} {} {} {}", pts[0].0, pts[0].1, pts[1].0, pts[1].1, pts[2].0, pts[2].1, pts[3].0, pts[3].1)).ok_or("dörtgen")?;
            let (p, org, err) = osisub_core::perspective::fit(&q, w, h, dist).ok_or("çözüm bulunamadı")?;
            eprintln!("en büyük köşe hatası {err:.3} px");
            println!("{}", osisub_core::perspective::tags(&p, org));
            Ok(())
        }
        "shape" => {
            // metni ASS çizimine çevir: shape "Metin" [--font Arial] [--size 64] [--bold] [--italic] [--fscx 100] [--fscy 100] [--fsp 0]
            let mut st = osisub_lua::StyleMetrics { fontname: opt(&mut args, "--font").unwrap_or_else(|| "Arial".into()), fontsize: opt(&mut args, "--size").and_then(|v| v.parse().ok()).unwrap_or(64.0), scale_x: 100.0, scale_y: 100.0, ..Default::default() };
            st.bold = flag(&mut args, "--bold");
            st.italic = flag(&mut args, "--italic");
            if let Some(v) = opt(&mut args, "--fscx") { st.scale_x = v.parse().map_err(|_| "--fscx sayı")?; }
            if let Some(v) = opt(&mut args, "--fscy") { st.scale_y = v.parse().map_err(|_| "--fscy sayı")?; }
            if let Some(v) = opt(&mut args, "--fsp") { st.spacing = v.parse().map_err(|_| "--fsp sayı")?; }
            let text = args.first().ok_or("metin gerekli")?;
            let mut db = osisub_lua::fonts::FontDb::default();
            let (d, w, h) = db.text_shape(&st, text).ok_or(format!("yazı tipi bulunamadı: {}", st.fontname))?;
            eprintln!("genişlik {w:.2}, yükseklik {h:.2}");
            println!("{d}");
            Ok(())
        }
        "merge" => {
            let out = opt(&mut args, "-o").or_else(|| opt(&mut args, "--out")).ok_or("çıktı gerekli: -o <dosya>")?;
            let mut o = osisub_core::merge::MergeOptions::default();
            if let Some(v) = opt(&mut args, "--offset") { o.offset_ms = v.parse().map_err(|_| "--offset ms olmalı")?; }
            o.sort = flag(&mut args, "--sort");
            o.skip_comments = flag(&mut args, "--no-comments");
            if args.len() < 2 { return Err("kullanım: merge <ana.ass> <ek.ass>... -o <çıktı>".into()); }
            let mut base = load(&args[0])?;
            for f in &args[1..] {
                let r = osisub_core::merge::merge_into(&mut base, &load(f)?, &o);
                println!("{}: {} satır, {} yeni stil, {} yazı tipi{}{}", f, r.lines, r.styles_added, r.fonts_added,
                    r.resampled_from.map(|(w, h)| format!(", {w}×{h}'den yeniden örneklendi")).unwrap_or_default(),
                    if r.styles_renamed.is_empty() { String::new() } else { format!(", yeniden adlandırılan: {}", r.styles_renamed.iter().map(|(a, b)| format!("{a}→{b}")).collect::<Vec<_>>().join(", ")) });
            }
            save(&base, &out)
        }
        "compare" => {
            if args.len() < 2 { return Err("kullanım: compare <eski> <yeni>".into()); }
            let (a, b) = (load(&args[0])?, load(&args[1])?);
            let ch = osisub_core::compare::compare(&a, &b);
            let line = |s: &Script, i: usize| match &s.entries[i] { Entry::Dialogue(d) => format!("[{}] {}", osisub_core::time::format_ass(d.start_time), d.text), _ => String::new() };
            for c in &ch {
                match c {
                    osisub_core::compare::Change::Added { new } => println!("+ {}", line(&b, *new)),
                    osisub_core::compare::Change::Removed { old } => println!("- {}", line(&a, *old)),
                    osisub_core::compare::Change::Modified { old, new, .. } => { println!("~ {}", line(&a, *old)); println!("  {}", line(&b, *new)); }
                }
            }
            println!("{} fark", ch.len());
            Ok(())
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
            let video = opt(&mut args, "--video");
            let s = load(args.first().ok_or("dosya gerekli")?)?;
            let mut issues = qc::check(&s, &r);
            if let Some(v) = video {
                // kareye dayalı denetimler: sahne değişimine yakınlık (≤3 kare), titreme
                let vp = Path::new(&v);
                let fr = osisub_media::frame_timestamps(vp).ok().and_then(osisub_core::vfr::Framerate::from_timestamps).or_else(|| osisub_media::probe(vp).ok().map(|i| osisub_core::vfr::Framerate::cfr(i.fps)).filter(|f| f.fps() > 0.0)).ok_or("videonun kare hızı okunamadı")?;
                let kf = osisub_media::keyframes(vp)?;
                issues.extend(qc::frame_issues(&s, &fr, &kf, 3));
                issues.sort_by_key(|x| x.entry);
            }
            for is in &issues {
                if let Entry::Dialogue(d) = &s.entries[is.entry] {
                    println!("{} [{}] {:?}: {}", is.entry + 1, osisub_core::time::format_ass(d.start_time), is.kind, is.message);
                }
            }
            println!("{} sorun", issues.len());
            if issues.is_empty() { Ok(()) } else { Err(format!("{} sorun bulundu", issues.len())) }
        }
        "fix" => {
            let all = !["--overlaps", "--dupes", "--empty", "--clean", "--sort", "--snap", "--merge-same", "--merge-short", "--recalc"].iter().any(|f| args.contains(&f.to_string()));
            let merge_same: Option<i64> = opt(&mut args, "--merge-same").and_then(|v| v.parse().ok());
            let merge_short: Option<usize> = opt(&mut args, "--merge-short").and_then(|v| v.parse().ok());
            let recalc: Option<f64> = opt(&mut args, "--recalc").and_then(|v| v.parse().ok());
            let (ov, du, em, cl, so, sn) = (flag(&mut args, "--overlaps"), flag(&mut args, "--dupes"), flag(&mut args, "--empty"), flag(&mut args, "--clean"), flag(&mut args, "--sort"), flag(&mut args, "--snap"));
            let fps: Option<f64> = opt(&mut args, "--fps").and_then(|v| v.parse().ok());
            let tc = load_timecodes(&mut args)?;
            let inp = args.first().ok_or("girdi gerekli")?.clone();
            let out = args.get(1).cloned().unwrap_or_else(|| inp.clone());
            let mut s = load(&inp)?;
            if all || so { qc::sort_by_start(&mut s); }
            if all || du { println!("yinelenen silindi: {}", qc::remove_duplicates(&mut s)); }
            if all || em { println!("boş silindi: {}", qc::remove_empty(&mut s)); }
            if all || cl { for d in s.dialogues_mut() { d.text = qc::clean_text(&d.text); } }
            if sn {
                match (&tc, fps) {
                    (Some(tc), _) => qc::snap_to_framerate(&mut s, tc),
                    (None, Some(f)) => qc::snap_to_frames(&mut s, f),
                    (None, None) => return Err("--snap için --fps ya da --timecodes gerekli".into()),
                }
            }
            if let Some(g) = merge_same { println!("aynı metin birleştirildi: {}", osisub_core::batch::merge_same_text(&mut s, g, None)); }
            if let Some(c) = merge_short { println!("kısa satır birleştirildi: {}", osisub_core::batch::merge_short_lines(&mut s, c, 250, None)); }
            if let Some(c) = recalc { println!("süre yeniden hesaplandı: {}", osisub_core::batch::recalc_durations(&mut s, c, 1000, 7000, 84, true, None)); }
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
        "fixes" => {
            let mut f = osisub_core::fixes::Fixes::default();
            f.ellipsis = flag(&mut args, "--ellipsis");
            f.sentence_case = flag(&mut args, "--sentence");
            f.remove_hi = flag(&mut args, "--hi");
            f.min_duration_ms = opt(&mut args, "--min-dur").and_then(|v| v.parse().ok());
            f.min_gap_ms = opt(&mut args, "--min-gap").and_then(|v| v.parse().ok());
            let (tags, notes) = (flag(&mut args, "--tags"), flag(&mut args, "--notes"));
            let inp = args.first().ok_or("girdi gerekli")?.clone();
            let out = args.get(1).cloned().unwrap_or_else(|| inp.clone());
            let mut s = load(&inp)?;
            let r = osisub_core::fixes::apply(&mut s, &f);
            println!("{} satır değişti, {} zamanlama düzeltildi", r.changed_lines, r.timing_changes);
            let mut tag_changes = 0;
            for e in s.entries.iter_mut() {
                let osisub_core::ass::Entry::Dialogue(d) = e else { continue };
                if d.comment { continue; }
                if notes {
                    let t = osisub_core::taglint::strip_notes(&d.text);
                    if t != d.text { d.text = t; tag_changes += 1; }
                }
                if tags && !d.effect.eq_ignore_ascii_case("fx") {
                    if let Some(t) = osisub_core::taglint::repair(d) { d.text = t; tag_changes += 1; }
                }
            }
            if tags || notes { println!("{} satırda etiket/not düzeltildi", tag_changes); }
            save(&s, &out)
        }
        "resample" => {
            if args.len() < 3 { return Err("kullanım: resample <girdi> <genişlik> <yükseklik> [çıktı]".into()); }
            let (w, h): (i32, i32) = (args[1].parse().map_err(|_| "genişlik sayı olmalı")?, args[2].parse().map_err(|_| "yükseklik sayı olmalı")?);
            let out = args.get(3).cloned().unwrap_or_else(|| args[0].clone());
            let mut s = load(&args[0])?;
            osisub_core::timing::resample(&mut s, w, h);
            save(&s, &out)
        }
        "post" => {
            let mut o = osisub_core::timing::PostOptions::default();
            if let Some(v) = opt(&mut args, "--lead-in") { o.lead_in_ms = v.parse().map_err(|_| "--lead-in sayı olmalı")?; }
            if let Some(v) = opt(&mut args, "--lead-out") { o.lead_out_ms = v.parse().map_err(|_| "--lead-out sayı olmalı")?; }
            if let Some(v) = opt(&mut args, "--gap") { o.adjacent_gap_ms = v.parse().map_err(|_| "--gap sayı olmalı")?; }
            if let Some(v) = opt(&mut args, "--video") {
                let vp = Path::new(&v);
                let kf = osisub_media::keyframes(vp)?;
                // ASS-güvenli kare sınırları (VFR algılanır, yoksa videonun sabit hızı)
                let fr = osisub_media::frame_timestamps(vp).ok().and_then(osisub_core::vfr::Framerate::from_timestamps).or_else(|| osisub_media::probe(vp).ok().map(|i| osisub_core::vfr::Framerate::cfr(i.fps)).filter(|f| f.fps() > 0.0));
                o.keyframes = match fr { Some(fr) => fr.keyframe_boundaries(&kf), None => kf };
                println!("{} anahtar kare", o.keyframes.len());
            }
            let inp = args.first().ok_or("girdi gerekli")?.clone();
            let out = args.get(1).cloned().unwrap_or_else(|| inp.clone());
            let mut s = load(&inp)?;
            osisub_core::timing::post_process(&mut s, &o);
            save(&s, &out)
        }
        "find" => {
            let regex = flag(&mut args, "--regex");
            let case = flag(&mut args, "--case");
            let repl = opt(&mut args, "--replace");
            if args.len() < 2 { return Err("kullanım: find <girdi> <desen> [--replace METİN [çıktı]]".into()); }
            let o = osisub_core::search::Options { pattern: args[1].clone(), replacement: repl.clone().unwrap_or_default(), regex, match_case: case, ..Default::default() };
            let mut s = load(&args[0])?;
            if repl.is_some() {
                let n = osisub_core::search::replace_all(&mut s, &o)?;
                println!("{} alan değiştirildi", n);
                save(&s, args.get(2).unwrap_or(&args[0]))
            } else {
                let hits = osisub_core::search::find_all(&s, &o)?;
                for h in &hits {
                    if let Entry::Dialogue(d) = &s.entries[h.entry] {
                        println!("{} [{}] {}", h.entry + 1, osisub_core::time::format_ass(d.start_time), d.text);
                    }
                }
                println!("{} eşleşme", hits.len());
                Ok(())
            }
        }
        "fonts" => {
            let s = load(args.first().ok_or("girdi gerekli")?)?;
            let mut db = osisub_lua::fonts::FontDb::default();
            let mut missing = 0;
            for u in osisub_core::fonts_used::used_fonts(&s) {
                let ok = db.has_family(&u.family);
                if !ok { missing += 1; }
                println!("{:<30} {}{} {:>4} satır  {}", u.family, if u.bold { "B" } else { "-" }, if u.italic { "I" } else { "-" }, u.lines, if ok { "kurulu" } else { "EKSİK" });
            }
            if missing > 0 { Err(format!("{} yazı tipi eksik", missing)) } else { Ok(()) }
        }
        "batch" => {
            let recursive = flag(&mut args, "--recursive");
            if args.len() < 2 { return Err("kullanım: batch <klasör> <uzantı> [--recursive]".into()); }
            let to = args[1].trim_start_matches('.').to_lowercase();
            let mut files = vec![];
            collect(Path::new(&args[0]), recursive, &mut files);
            let mut n = 0;
            for f in files {
                let ext = osisub_core::io::ext_of(&f);
                if !osisub_core::io::OPEN_EXTS.contains(&ext.as_str()) || ext == "txt" || ext == to { continue; }
                match osisub_core::io::load(&f).and_then(|s| osisub_core::io::save(&s, &f.with_extension(&to))) {
                    Ok(()) => { n += 1; println!("{} → .{}", f.display(), to); }
                    Err(e) => eprintln!("atlandı {}: {}", f.display(), e),
                }
            }
            println!("{} dosya dönüştürüldü", n);
            Ok(())
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
            let timecodes = load_timecodes(&mut args)?.map(std::sync::Arc::new);
            if args.len() < 3 { return Err("kullanım: run <girdi> <betik.lua> <makro> [çıktı]".into()); }
            let (inp, lua, mac) = (args[0].clone(), args[1].clone(), args[2].clone());
            let out = args.get(3).cloned().unwrap_or_else(|| inp.clone());
            let mut e = new_engine(HeadlessHost { params, fps, timecodes, ..Default::default() })?;
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

fn collect(dir: &Path, recursive: bool, out: &mut Vec<PathBuf>) {
    let Ok(rd) = std::fs::read_dir(dir) else { return };
    for e in rd.filter_map(|e| e.ok()) {
        let p = e.path();
        if p.is_dir() {
            if recursive { collect(&p, recursive, out); }
        } else {
            out.push(p);
        }
    }
}

fn new_engine(host: HeadlessHost) -> Result<Engine, String> {
    let mut e = Engine::new(Box::new(host)).map_err(|e| e.to_string())?;
    // GUI ile aynı arama sırası: osisub automation/include, kurulu Aegisub'ın include'u, programın yanındaki include
    for d in osisub_core::paths::automation_include_dirs() {
        e.add_include_dir(d);
    }
    if let Ok(exe) = std::env::current_exe() {
        if let Some(d) = exe.parent() {
            e.add_include_dir(PathBuf::from(d).join("include"));
        }
    }
    if let Ok(extra) = std::env::var("OSISUB_INCLUDE") {
        for d in std::env::split_paths(&extra) {
            e.add_include_dir(d);
        }
    }
    Ok(e)
}

/// `--timecodes dosya` seçeneğini okur (v1/v2).
fn load_timecodes(args: &mut Vec<String>) -> Result<Option<osisub_core::vfr::Framerate>, String> {
    match opt(args, "--timecodes") {
        Some(p) => {
            let t = std::fs::read_to_string(&p).map_err(|e| format!("{}: {}", p, e))?;
            osisub_core::vfr::Framerate::parse_timecodes(&t).map(Some).map_err(|e| format!("{}: {}", p, e))
        }
        None => Ok(None),
    }
}
