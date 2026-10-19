use clap::{Parser, Subcommand};
use std::cell::RefCell;
use std::path::PathBuf;
use std::process::ExitCode;
use std::rc::Rc;
use subforge_core::qc::{self, QcConfig, Severity};
use subforge_core::time::{format_ass, Fps};
use subforge_core::tools::{self, Case};
use subforge_core::{self as core, timing, Format, Script};
use subforge_lua::{Doc, Headless, LuaScript, ScriptError, SharedHost};

#[derive(Parser)]
#[command(name = "subforge", version, about = "SubForge — anime subtitle toolkit (Aegisub-compatible Lua automation)")]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Show statistics about a subtitle file
    Info { file: String },
    /// Convert between ass/srt/vtt (format picked from extension)
    Convert { input: String, output: String },
    /// Run quality checks; optionally apply automatic fixes
    Check {
        file: String,
        #[arg(long)]
        json: bool,
        /// Write the auto-fixed script here
        #[arg(long, short)]
        fix: Option<String>,
        #[arg(long, default_value_t = 21.0)]
        max_cps: f64,
        #[arg(long, default_value_t = 42)]
        max_line: usize,
        #[arg(long, default_value_t = 500)]
        min_dur: i64,
        #[arg(long, default_value_t = 0)]
        min_gap: i64,
    },
    /// Shift times by milliseconds (or frames with --fps)
    Shift {
        file: String,
        #[arg(long, short)]
        output: String,
        #[arg(long, allow_hyphen_values = true, default_value_t = 0)]
        ms: i64,
        #[arg(long, allow_hyphen_values = true)]
        frames: Option<i64>,
        #[arg(long)]
        fps: Option<f64>,
    },
    /// Two-point linear retime: a1→b1, a2→b2 (ms)
    Retime {
        file: String,
        #[arg(long, short)]
        output: String,
        #[arg(long)]
        a1: i64,
        #[arg(long)]
        b1: i64,
        #[arg(long)]
        a2: i64,
        #[arg(long)]
        b2: i64,
    },
    /// Convert timing from one frame rate to another
    Fps {
        file: String,
        #[arg(long, short)]
        output: String,
        #[arg(long)]
        from: f64,
        #[arg(long)]
        to: f64,
    },
    /// Close small gaps between lines
    Chain {
        file: String,
        #[arg(long, short)]
        output: String,
        #[arg(long, default_value_t = 300)]
        max_gap: i64,
    },
    /// Repair a file saved in a legacy Turkish encoding / broken UTF-8
    FixEncoding { file: String, #[arg(long, short)] output: String },
    /// Text cleanup on every dialogue line
    Text {
        file: String,
        #[arg(long, short)]
        output: String,
        /// upper | lower | title | sentence | fix | strip-hi | unbreak | balance | strip-tags
        #[arg(long)]
        op: String,
        #[arg(long, default_value_t = 42)]
        max: usize,
    },
    /// Automation 4 Lua scripts
    Lua {
        #[command(subcommand)]
        cmd: LuaCmd,
    },
}

#[derive(Subcommand)]
enum LuaCmd {
    /// List macros and filters in a script
    List { script: String },
    /// Run a macro on a subtitle file
    Run {
        script: String,
        file: String,
        /// macro name (or number from `lua list`)
        #[arg(long, short)]
        r#macro: String,
        #[arg(long, short)]
        output: String,
        /// Selected event numbers, 1-based, e.g. "1-10,15" (default: all)
        #[arg(long)]
        select: Option<String>,
        /// Dialog answers: name=value (repeatable)
        #[arg(long = "answer", short = 'a')]
        answers: Vec<String>,
        /// Button to press in dialogs
        #[arg(long)]
        button: Option<String>,
        #[arg(long)]
        fps: Option<f64>,
        /// Extra include dirs
        #[arg(long = "include", short = 'I')]
        include: Vec<PathBuf>,
    },
}

fn load(path: &str) -> Result<Script, String> {
    let (s, det) = core::load_path(path)?;
    if det.lossy {
        eprintln!("warning: {path}: some bytes could not be decoded ({})", det.name);
    }
    Ok(s)
}

fn parse_sel(spec: &str, max: usize) -> Vec<usize> {
    let mut v = vec![];
    for part in spec.split(',') {
        let part = part.trim();
        if let Some((a, b)) = part.split_once('-') {
            let (a, b): (usize, usize) = (a.trim().parse().unwrap_or(1), b.trim().parse().unwrap_or(max));
            v.extend(a.max(1)..=b.min(max));
        } else if let Ok(n) = part.parse::<usize>() {
            if n >= 1 && n <= max {
                v.push(n);
            }
        }
    }
    v
}

fn run() -> Result<ExitCode, String> {
    let cli = Cli::parse();
    match cli.cmd {
        Cmd::Info { file } => {
            let s = load(&file)?;
            let dlg = s.events.iter().filter(|e| !e.is_comment()).count();
            let (x, y) = s.play_res();
            let dur = s.events.iter().map(|e| e.end).max().unwrap_or(0);
            println!("file:      {file}");
            println!("title:     {}", s.info_get("Title").unwrap_or("-"));
            println!("resolution: {x}x{y}");
            println!("styles:    {}", s.styles.len());
            println!("events:    {} ({} dialogue, {} comment)", s.events.len(), dlg, s.events.len() - dlg);
            println!("last end:  {}", format_ass(dur));
            if let Some(v) = s.garbage_get("Video File") {
                println!("video:     {v}");
            }
        }
        Cmd::Convert { input, output } => {
            let s = load(&input)?;
            core::save_path(&s, &output)?;
        }
        Cmd::Check { file, json, fix, max_cps, max_line, min_dur, min_gap } => {
            let mut s = load(&file)?;
            let cfg = QcConfig { max_cps, max_line_chars: max_line, min_duration_ms: min_dur, min_gap_ms: min_gap, ..Default::default() };
            if let Some(out) = fix {
                let n = qc::autofix(&mut s, &cfg);
                core::save_path(&s, &out)?;
                eprintln!("applied {n} fixes → {out}");
            }
            let issues = qc::check(&s, &cfg);
            if json {
                println!("{}", serde_json::to_string_pretty(&issues).unwrap());
            } else {
                for i in &issues {
                    let e = &s.events[i.event];
                    let sev = match i.severity {
                        Severity::Error => "error",
                        Severity::Warning => "warn ",
                        Severity::Info => "info ",
                    };
                    println!("#{:<5} {} {} [{:?}] {}", i.event + 1, format_ass(e.start), sev, i.kind, i.message);
                }
                eprintln!("{} issue(s)", issues.len());
            }
            if issues.iter().any(|i| i.severity == Severity::Error) {
                return Ok(ExitCode::from(1));
            }
        }
        Cmd::Shift { file, output, ms, frames, fps } => {
            let mut s = load(&file)?;
            let idx: Vec<usize> = (0..s.events.len()).collect();
            match (frames, fps) {
                (Some(f), Some(r)) => timing::shift_frames(&mut s.events, &idx, f, Fps::from_f64(r)),
                (Some(_), None) => return Err("--frames needs --fps".into()),
                _ => timing::shift(&mut s.events, &idx, ms, true, true),
            }
            core::save_path(&s, &output)?;
        }
        Cmd::Retime { file, output, a1, b1, a2, b2 } => {
            let mut s = load(&file)?;
            let idx: Vec<usize> = (0..s.events.len()).collect();
            timing::linear_retime(&mut s.events, &idx, a1, b1, a2, b2);
            core::save_path(&s, &output)?;
        }
        Cmd::Fps { file, output, from, to } => {
            let mut s = load(&file)?;
            let idx: Vec<usize> = (0..s.events.len()).collect();
            timing::convert_fps(&mut s.events, &idx, Fps::from_f64(from), Fps::from_f64(to));
            core::save_path(&s, &output)?;
        }
        Cmd::Chain { file, output, max_gap } => {
            let mut s = load(&file)?;
            let idx: Vec<usize> = (0..s.events.len()).filter(|&i| !s.events[i].is_comment()).collect();
            let n = timing::chain(&mut s.events, &idx, max_gap, 0);
            eprintln!("closed {n} gap(s)");
            core::save_path(&s, &output)?;
        }
        Cmd::FixEncoding { file, output } => {
            let mut s = load(&file)?;
            let n = qc::autofix(&mut s, &QcConfig { check_overlaps: false, check_whitespace: false, check_empty: false, ..Default::default() });
            eprintln!("{n} line(s) repaired");
            core::save_path(&s, &output)?;
        }
        Cmd::Text { file, output, op, max } => {
            let mut s = load(&file)?;
            let f: Box<dyn Fn(&str) -> String> = match op.as_str() {
                "upper" => Box::new(|t| tools::change_case(t, Case::Upper)),
                "lower" => Box::new(|t| tools::change_case(t, Case::Lower)),
                "title" => Box::new(|t| tools::change_case(t, Case::Title)),
                "sentence" => Box::new(|t| tools::change_case(t, Case::Sentence)),
                "fix" => Box::new(|t| tools::fix_common(t, &tools::FixOptions::default())),
                "strip-hi" => Box::new(tools::strip_hearing_impaired),
                "unbreak" => Box::new(tools::unbreak),
                "balance" => Box::new(move |t| tools::balance_lines(t, max)),
                "strip-tags" => Box::new(tools::remove_all_tags),
                other => return Err(format!("unknown --op '{other}'")),
            };
            let mut n = 0;
            for e in s.events.iter_mut().filter(|e| !e.is_comment()) {
                let new = f(&e.text);
                if new != e.text {
                    e.text = new;
                    n += 1;
                }
            }
            eprintln!("{n} line(s) changed");
            core::save_path(&s, &output)?;
        }
        Cmd::Lua { cmd } => match cmd {
            LuaCmd::List { script } => {
                let host: SharedHost = Rc::new(RefCell::new(Headless::default()));
                let s = LuaScript::load(&PathBuf::from(&script), host, &subforge_app::settings::Settings::load().include_dirs());
                if let Some(e) = &s.load_error {
                    return Err(format!("failed to load {script}: {e}"));
                }
                println!("{} {} {}", s.name, s.version, if s.author.is_empty() { String::new() } else { format!("by {}", s.author) });
                for (i, m) in s.macros.iter().enumerate() {
                    println!("  macro #{i}: {} — {}", m.name, m.description);
                }
                for (i, f) in s.filters.iter().enumerate() {
                    println!("  filter #{i}: {} — {}", f.name, f.description);
                }
            }
            LuaCmd::Run { script, file, r#macro, output, select, answers, button, fps, include } => {
                let script_in = load(&file)?;
                let mut h = Headless { fps, button, script_path: Some(PathBuf::from(&file)), ..Default::default() };
                h.video_size = Some(script_in.play_res());
                for a in answers {
                    if let Some((k, v)) = a.split_once('=') {
                        h.answers.insert(k.to_string(), v.to_string());
                    }
                }
                let host: SharedHost = Rc::new(RefCell::new(h));
                let mut include = include;
                include.extend(subforge_app::settings::Settings::load().include_dirs());
                let s = LuaScript::load(&PathBuf::from(&script), host, &include);
                if let Some(e) = &s.load_error {
                    return Err(format!("failed to load {script}: {e}"));
                }
                let idx = match r#macro.parse::<usize>() {
                    Ok(n) if n < s.macros.len() => n,
                    _ => s.macros.iter().position(|m| m.name == r#macro).ok_or_else(|| format!("macro '{}' not found; try `subforge lua list`", r#macro))?,
                };
                let doc = Rc::new(RefCell::new(Doc::from_script(&script_in)));
                let n_ev = script_in.events.len();
                let sel_ev = select.map(|sp| parse_sel(&sp, n_ev)).unwrap_or_else(|| (1..=n_ev).collect());
                let sel: Vec<usize> = sel_ev.iter().map(|&e| doc.borrow().event_to_flat(e - 1)).collect();
                let active = sel.first().copied().unwrap_or(1);
                if !s.validate_macro(idx, doc.clone(), &sel, active) {
                    return Err("macro validation failed for this selection".into());
                }
                match s.run_macro(idx, doc.clone(), &sel, active) {
                    Ok(_) => {}
                    Err(ScriptError::Cancelled) => {
                        eprintln!("macro cancelled; nothing written");
                        return Ok(ExitCode::from(2));
                    }
                    Err(ScriptError::Failed(e)) => return Err(format!("script error: {e}")),
                }
                let out = doc.borrow().to_script();
                core::save_path(&out, &output)?;
                eprintln!("wrote {output} ({} events)", out.events.len());
            }
        },
    }
    let _ = Format::Ass;
    Ok(ExitCode::SUCCESS)
}

fn main() -> ExitCode {
    match run() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::from(1)
        }
    }
}
