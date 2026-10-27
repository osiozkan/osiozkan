use clap::{Parser, Subcommand};
use kaku_core::{formats, ops, qc, ass::Script, time::Fps};
use kaku_lua::{Kind, Runtime};
use std::cell::RefCell;
use std::path::PathBuf;
use std::process::ExitCode;
use std::rc::Rc;

#[derive(Parser)]
#[command(name = "kaku", version, about = "Kaku — anime subtitle toolkit (Aegisub-compatible automation, Subtitle Edit style tools)")]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Show script summary
    Info { file: PathBuf },
    /// Convert between .ass/.ssa/.srt/.vtt
    Convert { input: PathBuf, output: PathBuf },
    /// Run quality checks (CPS, overlaps, line length, missing styles…)
    Check {
        file: PathBuf,
        #[arg(long, default_value_t = 20.0)]
        cps: f64,
        #[arg(long, default_value_t = 42)]
        max_chars: usize,
        #[arg(long, default_value_t = 500)]
        min_dur: i64,
        #[arg(long, default_value_t = 7000)]
        max_dur: i64,
        #[arg(long, default_value_t = 0)]
        min_gap: i64,
    },
    /// Shift all timings by N milliseconds (negative allowed)
    Shift { file: PathBuf, ms: i64, #[arg(short, long)] output: Option<PathBuf> },
    /// Apply common fixes (spaces, empty lines, duplicates, sort)
    Fix { file: PathBuf, #[arg(short, long)] output: Option<PathBuf> },
    /// Change frame rate (e.g. 23.976 -> 25)
    Fps { file: PathBuf, from: f64, to: f64, #[arg(short, long)] output: Option<PathBuf> },
    /// Resample the script to a new resolution (scales styles, margins, \pos, \fs, drawings, …)
    Resample { file: PathBuf, #[arg(long)] x: i32, #[arg(long)] y: i32, #[arg(short, long)] output: Option<PathBuf> },
    /// Break long lines into balanced rows (lines containing override tags are skipped)
    Balance { file: PathBuf, #[arg(long, default_value_t = 42)] max: usize, #[arg(short, long)] output: Option<PathBuf> },
    /// Change text casing: upper | lower | sentence | title
    Case { file: PathBuf, mode: String, #[arg(long)] tr: bool, #[arg(short, long)] output: Option<PathBuf> },
    /// Remove hearing-impaired text ([noise], (laughs), ♪ lyrics, SPEAKER:)
    Hi { file: PathBuf, #[arg(short, long)] output: Option<PathBuf> },
    /// Timing post-processor: lead-in/out and joining of close lines
    Tpp { file: PathBuf, #[arg(long, default_value_t = 120)] lead_in: i64, #[arg(long, default_value_t = 400)] lead_out: i64, #[arg(long, default_value_t = 300)] join: i64, #[arg(short, long)] output: Option<PathBuf> },
    /// Show video/audio info of a media file (needs ffprobe)
    Media { file: PathBuf },
    /// Snap line starts/ends to video keyframes (needs ffprobe)
    Snap {
        file: PathBuf,
        #[arg(long)]
        video: PathBuf,
        #[arg(long, default_value_t = 250)]
        before: i64,
        #[arg(long, default_value_t = 250)]
        after: i64,
        #[arg(short, long)]
        output: Option<PathBuf>,
    },
    /// Render one video frame (optionally with subtitles burned in) to a PNG (needs ffmpeg)
    Frame {
        video: PathBuf,
        /// Time in ms
        #[arg(long, default_value_t = 0)]
        time: i64,
        #[arg(long)]
        subs: Option<PathBuf>,
        #[arg(short, long)]
        output: PathBuf,
    },
    /// Apply the karaoke templater (Aegisub-compatible) to a script
    Template { file: PathBuf, #[arg(short, long)] output: Option<PathBuf>, #[arg(short = 'I', long = "include")] include: Vec<PathBuf> },
    /// List macros/filters registered by a Lua automation script
    Macros { script: PathBuf, #[arg(short = 'I', long = "include")] include: Vec<PathBuf> },
    /// Run an Aegisub-style Lua macro/filter on a subtitle file
    Run {
        script: PathBuf,
        file: PathBuf,
        /// Macro/filter name (omit to use the only one)
        #[arg(short, long)]
        macro_name: Option<String>,
        #[arg(short, long)]
        output: Option<PathBuf>,
        /// Selected line numbers (1-based, among all entries), comma separated; default: all dialogue
        #[arg(long)]
        select: Option<String>,
        /// Dialog control override name=value (repeatable)
        #[arg(long = "set")]
        set: Vec<String>,
        /// Dialog button to press (default: first)
        #[arg(long)]
        button: Option<String>,
        #[arg(short = 'I', long = "include")]
        include: Vec<PathBuf>,
        /// Video fps for frame functions, e.g. 23.976
        #[arg(long)]
        fps: Option<f64>,
    },
}

fn load(p: &PathBuf) -> Result<Script, String> {
    let b = std::fs::read(p).map_err(|e| format!("{}: {e}", p.display()))?;
    Ok(formats::load(&p.to_string_lossy(), &b))
}

fn save(p: &PathBuf, s: &Script) -> Result<(), String> {
    std::fs::write(p, formats::save(&p.to_string_lossy(), s)).map_err(|e| format!("{}: {e}", p.display()))
}

fn fps_of(v: f64) -> Fps {
    for (n, d) in [(24000, 1001), (30000, 1001), (60000, 1001), (24, 1), (25, 1), (30, 1), (50, 1), (60, 1)] {
        if (n as f64 / d as f64 - v).abs() < 0.005 {
            return Fps::new(n, d);
        }
    }
    Fps::new((v * 1000.0).round() as u32, 1000)
}

fn run() -> Result<ExitCode, String> {
    let cli = Cli::parse();
    match cli.cmd {
        Cmd::Info { file } => {
            let s = load(&file)?;
            let (x, y) = s.play_res();
            let dur = s.lines().map(|l| l.end).max().unwrap_or(0);
            println!("lines: {}  styles: {}  resolution: {}x{}  length: {}", s.line_count(), s.styles().count(), x, y, kaku_core::time::format_ass_time(dur));
            for st in s.styles() {
                let used = s.lines().filter(|l| l.style == st.name).count();
                println!("  style {:<20} {} {}  ({} lines)", st.name, st.fontname, st.fontsize, used);
            }
        }
        Cmd::Convert { input, output } => {
            let s = load(&input)?;
            save(&output, &s)?;
            println!("{} -> {}", input.display(), output.display());
        }
        Cmd::Check { file, cps, max_chars, min_dur, max_dur, min_gap } => {
            let s = load(&file)?;
            let cfg = qc::QcConfig { max_cps: cps, max_line_chars: max_chars, min_duration: min_dur, max_duration: max_dur, min_gap, ..Default::default() };
            let issues = qc::check(&s, &cfg);
            let lines: Vec<_> = s.lines().collect();
            for i in &issues {
                println!("#{:<5} {:>8} {:<18} {}", i.line + 1, kaku_core::time::format_ass_time(lines[i.line].start), i.code, i.message);
            }
            println!("{} issue(s)", issues.len());
            if issues.iter().any(|i| i.severity == qc::Severity::Error) {
                return Ok(ExitCode::from(2));
            }
        }
        Cmd::Shift { file, ms, output } => {
            let mut s = load(&file)?;
            ops::shift(&mut s, ms, None);
            save(&output.unwrap_or(file), &s)?;
        }
        Cmd::Fix { file, output } => {
            let mut s = load(&file)?;
            let a = ops::fix_common_errors(&mut s);
            let b = ops::remove_empty(&mut s);
            let c = ops::remove_duplicates(&mut s);
            s.sort_by_start();
            println!("fixed text: {a}, removed empty: {b}, removed duplicates: {c}");
            save(&output.unwrap_or(file), &s)?;
        }
        Cmd::Fps { file, from, to, output } => {
            let mut s = load(&file)?;
            ops::change_fps(&mut s, fps_of(from), fps_of(to));
            save(&output.unwrap_or(file), &s)?;
        }
        Cmd::Resample { file, x, y, output } => {
            let mut s = load(&file)?;
            if !kaku_core::resample::resample(&mut s, x, y, Default::default()) {
                println!("already {x}x{y} (or source resolution unknown)");
            }
            save(&output.unwrap_or(file), &s)?;
        }
        Cmd::Balance { file, max, output } => {
            let mut s = load(&file)?;
            let n = kaku_core::textfix::balance_script(&mut s, max);
            println!("{n} line(s) rebalanced");
            save(&output.unwrap_or(file), &s)?;
        }
        Cmd::Case { file, mode, tr, output } => {
            use kaku_core::textfix::{change_case, Casing};
            let m = match mode.as_str() {
                "upper" => Casing::Upper,
                "lower" => Casing::Lower,
                "sentence" => Casing::Sentence,
                "title" => Casing::Title,
                _ => return Err("mode must be upper|lower|sentence|title".into()),
            };
            let mut s = load(&file)?;
            for l in s.lines_mut() {
                if !kaku_core::tags::has_drawing(&l.text) {
                    l.text = change_case(&l.text, m, tr);
                }
            }
            save(&output.unwrap_or(file), &s)?;
        }
        Cmd::Hi { file, output } => {
            let mut s = load(&file)?;
            let n = kaku_core::textfix::remove_hearing_impaired(&mut s);
            let e = ops::remove_empty(&mut s);
            println!("{n} line(s) changed, {e} emptied line(s) removed");
            save(&output.unwrap_or(file), &s)?;
        }
        Cmd::Tpp { file, lead_in, lead_out, join, output } => {
            let mut s = load(&file)?;
            let n = kaku_core::textfix::timing_post_process(&mut s, kaku_core::textfix::Tpp { lead_in, lead_out, join_gap: join });
            println!("{n} adjustment(s)");
            save(&output.unwrap_or(file), &s)?;
        }
        Cmd::Media { file } => {
            let i = kaku_media::probe(&file).map_err(|e| e.to_string())?;
            println!("duration: {}", kaku_core::time::format_ass_time(i.duration_ms));
            if i.has_video {
                println!("video: {}x{} @ {}", i.width, i.height, i.fps.map(|(n, d)| format!("{:.3} fps", n as f64 / d as f64)).unwrap_or("?".into()));
            }
            for a in &i.audio {
                println!("audio #{}: {} {} ch {} Hz {}", a.index, a.codec, a.channels, a.sample_rate, a.lang);
            }
        }
        Cmd::Snap { file, video, before, after, output } => {
            let mut s = load(&file)?;
            let kf = kaku_media::keyframes(&video).map_err(|e| e.to_string())?;
            let n = kaku_media::snap_to_keyframes(&mut s, &kf, before, after);
            println!("{n} line(s) snapped ({} keyframes)", kf.len());
            save(&output.unwrap_or(file), &s)?;
        }
        Cmd::Frame { video, time, subs, output } => {
            let png = kaku_media::render_frame(&video, time, subs.as_deref(), None, 0).map_err(|e| e.to_string())?;
            std::fs::write(&output, png).map_err(|e| e.to_string())?;
        }
        Cmd::Template { file, output, include } => {
            let rt = Runtime::new().map_err(|e| e.to_string())?;
            for d in include {
                rt.add_include_dir(d);
            }
            let regs = rt.load_builtins().map_err(|e| e.to_string())?;
            let s = load(&file)?;
            let (s, out) = kaku_lua::run_by_name(&rt, &regs, "Apply karaoke template", s, vec![]).map_err(|e| e.to_string())?;
            for l in &out.log {
                eprintln!("{l}");
            }
            save(&output.unwrap_or(file), &s)?;
        }
        Cmd::Macros { script, include } => {
            let rt = Runtime::new().map_err(|e| e.to_string())?;
            for d in include {
                rt.add_include_dir(d);
            }
            let regs = rt.load_script(&script).map_err(|e| e.to_string())?;
            for r in regs {
                println!("{:<6} {} — {}", if r.kind == Kind::Macro { "macro" } else { "filter" }, r.name, r.description);
            }
        }
        Cmd::Run { script, file, macro_name, output, select, set, button, include, fps } => {
            let rt = Runtime::new().map_err(|e| e.to_string())?;
            for d in include {
                rt.add_include_dir(d);
            }
            for kv in set {
                let (k, v) = kv.split_once('=').ok_or("--set expects name=value")?;
                rt.dialog_values.borrow_mut().insert(k.to_string(), v.to_string());
            }
            *rt.dialog_button.borrow_mut() = button;
            if let Some(f) = fps {
                let fp = fps_of(f);
                rt.set_video(kaku_lua::VideoInfo { fps: Some((fp.num, fp.den)), size: None });
            }
            let regs = rt.load_script(&script).map_err(|e| e.to_string())?;
            let reg = match &macro_name {
                Some(n) => regs.iter().find(|r| &r.name == n).ok_or(format!("macro '{n}' not found; available: {}", regs.iter().map(|r| r.name.clone()).collect::<Vec<_>>().join(", ")))?,
                None if regs.len() == 1 => &regs[0],
                None => return Err(format!("specify --macro-name; available: {}", regs.iter().map(|r| r.name.clone()).collect::<Vec<_>>().join(", "))),
            };
            let s = load(&file)?;
            let sel: Vec<usize> = match select {
                Some(t) => t.split(',').filter_map(|x| x.trim().parse().ok()).collect(),
                None => s.entries.iter().enumerate().filter(|(_, e)| matches!(e, kaku_core::ass::Entry::Line(_))).map(|(i, _)| i + 1).collect(),
            };
            let shared = Rc::new(RefCell::new(s));
            let active = sel.first().copied().unwrap_or(1);
            let res = rt.run(reg.index, &shared, &sel, active);
            match res {
                Ok(out) => {
                    for l in &out.log {
                        eprintln!("{l}");
                    }
                    if out.cancelled {
                        eprintln!("macro cancelled");
                        return Ok(ExitCode::from(3));
                    }
                    save(&output.unwrap_or(file), &shared.borrow())?;
                }
                Err(e) => return Err(e.to_string()),
            }
        }
    }
    Ok(ExitCode::SUCCESS)
}

fn main() -> ExitCode {
    match run() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}
