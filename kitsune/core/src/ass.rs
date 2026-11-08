//! ASS / SSA reader & writer (Aegisub-compatible output).
use crate::model::*;
use crate::time::{ass_time, parse_ass_time};

fn strip_bom(s: &str) -> &str { s.strip_prefix('\u{feff}').unwrap_or(s) }

fn pf(s: &str) -> f64 { s.trim().parse().unwrap_or(0.0) }
fn pi(s: &str) -> i32 { s.trim().parse::<f64>().map(|v| v as i32).unwrap_or(0) }
fn pb(s: &str) -> bool { s.trim().parse::<i32>().map(|v| v != 0).unwrap_or(false) }

/// SSA colors/alignments are converted to ASS conventions on load.
fn norm_color(s: &str) -> String {
    let t = s.trim();
    if let Some(h) = t.strip_prefix("&H").or_else(|| t.strip_prefix("&h")) {
        let h = h.trim_end_matches('&');
        if let Ok(v) = u32::from_str_radix(h, 16) { return format!("&H{:08X}", v); }
    }
    if let Ok(v) = t.parse::<i64>() { return format!("&H{:08X}", (v as u32)); }
    "&H00FFFFFF".into()
}

fn ssa_align(a: i32) -> i32 {
    // SSA: 1-3 bottom, 5-7 top, 9-11 middle
    match a { 1..=3 => a, 5..=7 => a - 5 + 7, 9..=11 => a - 9 + 4, _ => 2 }
}

pub fn parse(input: &str) -> Script {
    let input = strip_bom(input);
    let mut script = Script::default();
    let mut section = String::new();
    let mut style_fmt: Vec<String> = Vec::new();
    let mut ev_fmt: Vec<String> = Vec::new();
    let mut ssa = false;
    for line in input.lines() {
        let raw = line.trim_end_matches('\r');
        let t = raw.trim();
        if t.starts_with('[') && t.ends_with(']') {
            section = t[1..t.len() - 1].to_string();
            continue;
        }
        if t.is_empty() { continue; }
        let sl = section.to_ascii_lowercase();
        match sl.as_str() {
            "script info" => {
                if t.starts_with(';') || t.starts_with('!') {
                    script.entries.push(Entry::Raw { section: section.clone(), raw: raw.into() });
                } else if let Some((k, v)) = t.split_once(':') {
                    let k = k.trim();
                    let v = v.trim();
                    if k.eq_ignore_ascii_case("ScriptType") && v.eq_ignore_ascii_case("v4.00") { ssa = true; }
                    script.entries.push(Entry::Info { key: k.into(), value: v.into() });
                }
            }
            "v4+ styles" | "v4 styles" | "v4 styles+" => {
                if sl == "v4 styles" { ssa = true; }
                if let Some(rest) = t.strip_prefix("Format:") {
                    style_fmt = rest.split(',').map(|s| s.trim().to_ascii_lowercase()).collect();
                } else if let Some(rest) = t.strip_prefix("Style:") {
                    if style_fmt.is_empty() {
                        style_fmt = "name,fontname,fontsize,primarycolour,secondarycolour,outlinecolour,backcolour,bold,italic,underline,strikeout,scalex,scaley,spacing,angle,borderstyle,outline,shadow,alignment,marginl,marginr,marginv,encoding".split(',').map(String::from).collect();
                    }
                    script.entries.push(Entry::Style(parse_style(rest, &style_fmt, ssa)));
                }
            }
            "events" => {
                if let Some(rest) = t.strip_prefix("Format:") {
                    ev_fmt = rest.split(',').map(|s| s.trim().to_ascii_lowercase()).collect();
                } else {
                    let (comment, rest) = if let Some(r) = t.strip_prefix("Dialogue:") { (false, r) }
                        else if let Some(r) = t.strip_prefix("Comment:") { (true, r) }
                        else { continue };
                    if ev_fmt.is_empty() {
                        ev_fmt = "layer,start,end,style,name,marginl,marginr,marginv,effect,text".split(',').map(String::from).collect();
                    }
                    if let Some(ev) = parse_event(rest, &ev_fmt, comment) { script.entries.push(Entry::Event(ev)); }
                }
            }
            _ => script.entries.push(Entry::Raw { section: section.clone(), raw: raw.into() }),
        }
    }
    if ssa {
        script.set_info("ScriptType", "v4.00+");
    }
    script
}

fn parse_style(rest: &str, fmt: &[String], ssa: bool) -> Style {
    let n = fmt.len();
    let vals: Vec<&str> = rest.trim_start().splitn(n, ',').collect();
    let mut s = Style::default();
    for (k, v) in fmt.iter().zip(vals.iter()) {
        match k.as_str() {
            "name" => s.name = v.trim().into(),
            "fontname" => s.fontname = v.trim().into(),
            "fontsize" => s.fontsize = pf(v),
            "primarycolour" => s.color1 = norm_color(v),
            "secondarycolour" => s.color2 = norm_color(v),
            "outlinecolour" | "tertiarycolour" => s.color3 = norm_color(v),
            "backcolour" => s.color4 = norm_color(v),
            "bold" => s.bold = pb(v),
            "italic" => s.italic = pb(v),
            "underline" => s.underline = pb(v),
            "strikeout" => s.strikeout = pb(v),
            "scalex" => s.scale_x = pf(v),
            "scaley" => s.scale_y = pf(v),
            "spacing" => s.spacing = pf(v),
            "angle" => s.angle = pf(v),
            "borderstyle" => s.borderstyle = pi(v),
            "outline" => s.outline = pf(v),
            "shadow" => s.shadow = pf(v),
            "alignment" => { let a = pi(v); s.align = if ssa { ssa_align(a) } else { a } }
            "marginl" => s.margin_l = pi(v),
            "marginr" => s.margin_r = pi(v),
            "marginv" => s.margin_t = pi(v),
            "encoding" => s.encoding = pi(v),
            _ => {}
        }
    }
    s
}

fn parse_event(rest: &str, fmt: &[String], comment: bool) -> Option<Event> {
    let n = fmt.len();
    let vals: Vec<&str> = rest.trim_start().splitn(n, ',').collect();
    if vals.len() < n { return None; }
    let mut e = Event { comment, ..Default::default() };
    for (k, v) in fmt.iter().zip(vals.iter()) {
        match k.as_str() {
            "layer" | "marked" => e.layer = pi(&v.replace("Marked=", "")),
            "start" => e.start = parse_ass_time(v)?,
            "end" => e.end = parse_ass_time(v)?,
            "style" => e.style = v.trim().trim_start_matches('*').into(),
            "name" | "actor" => e.actor = v.trim().into(),
            "marginl" => e.margin_l = pi(v),
            "marginr" => e.margin_r = pi(v),
            "marginv" | "margint" => e.margin_t = pi(v),
            "effect" => e.effect = v.trim().into(),
            "text" => e.text = (*v).into(),
            _ => {}
        }
    }
    Some(e)
}

fn b(v: bool) -> i32 { if v { -1 } else { 0 } }

fn num(v: f64) -> String {
    if v.fract() == 0.0 { format!("{}", v as i64) } else {
        let s = format!("{:.3}", v);
        s.trim_end_matches('0').trim_end_matches('.').to_string()
    }
}

pub fn write(script: &Script) -> String {
    let mut out = String::from("\u{feff}");
    out.push_str("[Script Info]\n");
    let mut sections: Vec<String> = Vec::new();
    for e in &script.entries {
        match e {
            Entry::Info { key, value } => out.push_str(&format!("{}: {}\n", key, value)),
            Entry::Raw { section, raw } => {
                if section.eq_ignore_ascii_case("Script Info") { out.push_str(raw); out.push('\n'); }
                else if !sections.contains(section) { sections.push(section.clone()); }
            }
            _ => {}
        }
    }
    out.push_str("\n[V4+ Styles]\nFormat: Name, Fontname, Fontsize, PrimaryColour, SecondaryColour, OutlineColour, BackColour, Bold, Italic, Underline, StrikeOut, ScaleX, ScaleY, Spacing, Angle, BorderStyle, Outline, Shadow, Alignment, MarginL, MarginR, MarginV, Encoding\n");
    for s in script.styles() {
        out.push_str(&format!(
            "Style: {},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{}\n",
            s.name, s.fontname, num(s.fontsize), s.color1, s.color2, s.color3, s.color4,
            b(s.bold), b(s.italic), b(s.underline), b(s.strikeout), num(s.scale_x), num(s.scale_y),
            num(s.spacing), num(s.angle), s.borderstyle, num(s.outline), num(s.shadow), s.align,
            s.margin_l, s.margin_r, s.margin_t, s.encoding));
    }
    out.push_str("\n[Events]\nFormat: Layer, Start, End, Style, Name, MarginL, MarginR, MarginV, Effect, Text\n");
    for e in script.events() {
        out.push_str(&format!("{}: {},{},{},{},{},{},{},{},{},{}\n",
            if e.comment { "Comment" } else { "Dialogue" }, e.layer, ass_time(e.start), ass_time(e.end),
            e.style, e.actor, e.margin_l, e.margin_r, e.margin_t, e.effect, e.text));
    }
    for sec in sections {
        out.push_str(&format!("\n[{}]\n", sec));
        for e in &script.entries {
            if let Entry::Raw { section, raw } = e { if *section == sec { out.push_str(raw); out.push('\n'); } }
        }
    }
    out
}
