//! `aegisub.*` API'sinin yerel (Rust) tarafı ve gömülü Lua kütüphaneleri.
use crate::dialog::{DialogSpec, DV};
use crate::engine::{Inner, MacroDef, MacroInfo, MacroKind, StyleMetrics, CANCEL_MARKER};
use crate::subs::SharedScript;
use fancy_regex::{Regex, RegexBuilder};
use mlua::{Function, Lua, MultiValue, Result, Table, Value, Variadic};
use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;

type InnerRc = Rc<RefCell<Inner>>;

const UTILS: &str = include_str!("../lua/utils.lua");
const KARASKEL: &str = include_str!("../lua/karaskel.lua");
const UTF8_LUA: &str = include_str!("../lua/utf8.lua");
const UTILS_AUTO4: &str = include_str!("../lua/utils-auto4.lua");
const KARASKEL_AUTO4: &str = include_str!("../lua/karaskel-auto4.lua");
const CLEANTAGS: &str = include_str!("../lua/cleantags.lua");
const UNICODE_LUA: &str = "unicode = aegisub.__unicode\nreturn unicode\n";

fn embedded(name: &str) -> Option<&'static str> {
    match name.rsplit(['/', '\\']).next().unwrap_or(name) {
        "utils.lua" => Some(UTILS),
        "karaskel.lua" => Some(KARASKEL),
        "unicode.lua" => Some(UNICODE_LUA),
        "utf8.lua" => Some(UTF8_LUA),
        "utils-auto4.lua" => Some(UTILS_AUTO4),
        "karaskel-auto4.lua" => Some(KARASKEL_AUTO4),
        "cleantags.lua" => Some(CLEANTAGS),
        _ => None,
    }
}

pub fn dv_to_lua(lua: &Lua, v: &DV) -> Result<Value> {
    Ok(match v {
        DV::Bool(b) => Value::Boolean(*b),
        DV::Num(n) => Value::Number(*n),
        DV::Str(s) => Value::String(lua.create_string(s)?),
    })
}

/// Yazı tipi dosyası olmadan kabaca metin ölçüsü: (genişlik, yükseklik, descent, extlead).
pub fn approx_extents(st: &StyleMetrics, text: &str) -> (f64, f64, f64, f64) {
    let fs = st.fontsize.max(0.0);
    let mut w = 0.0;
    for c in text.chars() {
        let em = match c {
            ' ' => 0.28,
            'i' | 'l' | 'I' | 'j' | '.' | ',' | ':' | ';' | '|' | '!' | '\'' | '`' => 0.28,
            'f' | 't' | 'r' | '(' | ')' | '[' | ']' | '-' => 0.35,
            'm' | 'w' | 'M' | 'W' | '@' | '%' => 0.85,
            c if c.is_ascii_uppercase() => 0.68,
            c if (c as u32) >= 0x2E80 => 1.0,
            _ => 0.52,
        };
        w += em * fs * if st.bold { 1.07 } else { 1.0 } * st.scale_x / 100.0 + st.spacing * st.scale_x / 100.0;
    }
    (w, fs * 1.117 * st.scale_y / 100.0, fs * 0.212 * st.scale_y / 100.0, fs * 0.033)
}

fn style_metrics(t: &Table) -> StyleMetrics {
    let n = |k: &str, d: f64| match t.get::<Value>(k) {
        Ok(Value::Number(x)) => x,
        Ok(Value::Integer(x)) => x as f64,
        _ => d,
    };
    let b = |k: &str| matches!(t.get::<Value>(k), Ok(Value::Boolean(true))) || matches!(t.get::<Value>(k), Ok(Value::Number(x)) if x != 0.0);
    StyleMetrics {
        fontname: t.get::<String>("fontname").unwrap_or_default(),
        fontsize: n("fontsize", 20.0),
        bold: b("bold"),
        italic: b("italic"),
        scale_x: n("scale_x", 100.0),
        scale_y: n("scale_y", 100.0),
        spacing: n("spacing", 0.0),
    }
}

pub(crate) fn install(lua: &Lua, inner: &InnerRc, script: &SharedScript) -> Result<()> {
    let _ = script;
    let g = lua.globals();
    let aegisub = lua.create_table()?;
    let native = lua.create_table()?;

    // ---- yerel yardımcılar (prelude kullanır)
    {
        let inn = inner.clone();
        native.set(
            "find_include",
            lua.create_function(move |_, (name, dir): (String, String)| {
                let mut candidates: Vec<PathBuf> = vec![];
                if !dir.is_empty() {
                    candidates.push(PathBuf::from(&dir).join(&name));
                }
                for d in inn.borrow().include_dirs.iter() {
                    candidates.push(d.join(&name));
                }
                candidates.push(PathBuf::from(&name));
                for c in candidates {
                    if c.is_file() {
                        let bytes = std::fs::read(&c).map_err(mlua::Error::external)?;
                        let mut s = osisub_core::ass::decode_bytes(&bytes);
                        if s.starts_with('\u{feff}') {
                            s.remove(0);
                        }
                        return Ok((s, c.to_string_lossy().to_string()));
                    }
                }
                if let Some(e) = embedded(&name) {
                    return Ok((e.to_string(), format!("osisub/{}", name)));
                }
                Err(mlua::Error::runtime(format!("Could not find include file: {}", name)))
            })?,
        )?;
        native.set("embedded_module", lua.create_function(|_, name: String| Ok(crate::vendor::module(&name)))?)?;
        native.set("fallback_module", lua.create_function(|_, name: String| Ok(crate::vendor::fallback_module(&name)))?)?;
        let inn = inner.clone();
        // require için: include klasörlerinde a/b.lua, a/b/init.lua, a/b.moon, a/b/init.moon ara.
        native.set(
            "find_module",
            lua.create_function(move |_, name: String| {
                let rel = name.replace('.', "/");
                let roots: Vec<PathBuf> = inn.borrow().include_dirs.clone();
                for r in roots {
                    for cand in [format!("{rel}.lua"), format!("{rel}/init.lua"), format!("{rel}.moon"), format!("{rel}/init.moon")] {
                        let c = r.join(&cand);
                        if c.is_file() {
                            let bytes = std::fs::read(&c).map_err(mlua::Error::external)?;
                            let mut s = osisub_core::ass::decode_bytes(&bytes);
                            if s.starts_with('\u{feff}') {
                                s.remove(0);
                            }
                            return Ok((Some(s), Some(c.to_string_lossy().to_string())));
                        }
                    }
                }
                Ok((None, None))
            })?,
        )?;
        let inn = inner.clone();
        native.set(
            "log",
            lua.create_function(move |_, (level, msg): (i64, mlua::String)| {
                // bayt bayt kesilmiş dize (("é"):sub(1,1)) hata vermesin: Aegisub da yazar
                let msg = msg.to_string_lossy().to_string();
                let mut i = inn.borrow_mut();
                i.host.log(level, &msg);
                i.log.push_str(&msg);
                Ok(())
            })?,
        )?;
    }
    {
        // SubInspector karşılığı: satırları (Aegisub satır tabloları) betiğin başlığı ve stilleriyle,
        // video olmadan, verilen anlarda libass ile çizip sınırları döndürür.
        let inn = inner.clone();
        let sc = script.clone();
        native.set(
            "render_bounds",
            lua.create_function(move |lua, (lines, times): (Table, Table)| {
                let mut s = sc.borrow().clone();
                s.entries.retain(|e| !matches!(e, osisub_core::ass::Entry::Dialogue(_)));
                for t in lines.sequence_values::<Table>() {
                    let t = t?;
                    if t.get::<Option<String>>("class")?.is_none() {
                        t.set("class", "dialogue")?;
                    }
                    if let osisub_core::ass::Entry::Dialogue(mut d) = crate::subs::table_to_entry(&t, "Events")? {
                        d.comment = false;
                        s.entries.push(osisub_core::ass::Entry::Dialogue(d));
                    }
                }
                let times: Vec<i64> = times.sequence_values::<f64>().filter_map(|v| v.ok()).map(|v| v.round() as i64).collect();
                let (w, h) = s.script_resolution();
                let r = inn.borrow_mut().host.render_bounds(&s.to_ass_string(), w.max(1) as u32, h.max(1) as u32, &times);
                match r {
                    Ok(v) => {
                        let out = lua.create_table()?;
                        for (i, b) in v.into_iter().enumerate() {
                            match b {
                                Some(b) => {
                                    let t = lua.create_table()?;
                                    t.set("x", b.x)?;
                                    t.set("y", b.y)?;
                                    t.set("w", b.w)?;
                                    t.set("h", b.h)?;
                                    t.set("hash", b.hash)?;
                                    t.set("solid", b.solid)?;
                                    out.set(i + 1, t)?;
                                }
                                None => out.set(i + 1, false)?,
                            }
                        }
                        Ok((Value::Table(out), Value::Nil))
                    }
                    Err(e) => Ok((Value::Nil, Value::String(lua.create_string(&e)?))),
                }
            })?,
        )?;
    }
    {
        // duvar saati (saniye) ve uyku: PT.PreciseTimer karşılığı için
        let t0 = std::time::Instant::now();
        native.set("clock", lua.create_function(move |_, ()| Ok(t0.elapsed().as_secs_f64()))?)?;
        native.set(
            "sleep",
            lua.create_function(|_, ms: f64| {
                std::thread::sleep(std::time::Duration::from_secs_f64((ms.max(0.0) / 1000.0).min(60.0)));
                Ok(())
            })?,
        )?;
    }
    {
        // osisub'a özgü yardımcılar (`require "osisub"`): Aegisub'da olmayan, betiklerin
        // osisub çekirdeğini kullanabilmesi için saf metin işlevleri
        use osisub_core as c;
        native.set("tags_clean", lua.create_function(|_, t: String| Ok(c::tagclean::clean(&t).unwrap_or(t)))?)?;
        native.set("tags_strip_notes", lua.create_function(|_, t: String| Ok(c::taglint::strip_notes(&t)))?)?;
        native.set("tags_lint", lua.create_function(|lua, (t, dur): (String, Option<f64>)| {
            let d = c::ass::Dialogue { start_time: 0, end_time: dur.unwrap_or(5000.0) as i64, text: t, style: "Default".into(), ..Default::default() };
            let out = lua.create_table()?;
            for (i, f) in c::taglint::lint(&d, &["Default"]).into_iter().enumerate() {
                let (kind, m) = match f { c::taglint::Finding::Error(m) => ("error", m), c::taglint::Finding::Note(m) => ("note", m) };
                let e = lua.create_table()?;
                e.set("kind", kind)?;
                e.set("message", m)?;
                out.set(i + 1, e)?;
            }
            Ok(out)
        })?)?;
        native.set("case", lua.create_function(|_, (t, mode, tr): (String, String, Option<bool>)| {
            let k = match mode.as_str() { "upper" => c::casing::Case::Upper, "lower" => c::casing::Case::Lower, "title" => c::casing::Case::Title, "sentence" => c::casing::Case::Sentence, _ => return Err(mlua::Error::runtime("kip: upper|lower|title|sentence")) };
            Ok(c::casing::apply(&t, k, tr.unwrap_or(true)))
        })?)?;
        native.set("balance", lua.create_function(|_, (t, max): (String, Option<usize>)| Ok(c::linebreak::balance(&t, max.unwrap_or(42))))?)?;
        native.set("unbreak", lua.create_function(|_, t: String| Ok(c::linebreak::unbreak(&t)))?)?;
        native.set("tr_fix", lua.create_function(|_, t: String| Ok(c::trfix::fix_text(&t)))?)?;
        native.set("furigana_strip", lua.create_function(|_, t: String| Ok(c::furigana::strip(&t)))?)?;
        native.set("furigana_has", lua.create_function(|_, t: String| Ok(c::furigana::has_furigana(&t)))?)?;
        native.set("romaji_to_kana", lua.create_function(|_, t: String| Ok(c::kanjitimer::romaji_to_hiragana(&t)))?)?;
        native.set("visible_text", lua.create_function(|_, t: String| Ok(c::tags::visible_text(&t)))?)?;
        native.set("time_format", lua.create_function(|_, ms: f64| Ok(c::time::format_ass(if ms.is_finite() { ms.round().clamp(-3.6e11, 3.6e11) as i64 } else { 0 })))?)?;
        native.set("time_parse", lua.create_function(|_, t: String| Ok(c::time::parse_generic(&t)))?)?;
    }
    g.set("__osisub_native", native)?;

    // ---- kayıt
    {
        let inn = inner.clone();
        aegisub.set(
            "register_macro",
            lua.create_function(move |_, (name, desc, process, validate, is_active): (String, Option<String>, Function, Option<Function>, Option<Function>)| {
                let mut i = inn.borrow_mut();
                let id = i.macros.len();
                let file = i.current_script.clone();
                i.macros.push(MacroDef {
                    info: MacroInfo { id, name, description: desc.unwrap_or_default(), kind: MacroKind::Macro, priority: 0, script_file: file },
                    process,
                    validate,
                    is_active,
                    config: None,
                });
                Ok(())
            })?,
        )?;
        let inn = inner.clone();
        aegisub.set(
            "register_filter",
            lua.create_function(move |_, (name, desc, prio, process, config): (String, Option<String>, Option<i64>, Function, Option<Function>)| {
                let mut i = inn.borrow_mut();
                let id = i.macros.len();
                let file = i.current_script.clone();
                i.macros.push(MacroDef {
                    info: MacroInfo { id, name, description: desc.unwrap_or_default(), kind: MacroKind::Filter, priority: prio.unwrap_or(0), script_file: file },
                    process,
                    validate: None,
                    is_active: None,
                    config,
                });
                Ok(())
            })?,
        )?;
    }
    aegisub.set("cancel", lua.create_function(|_, ()| -> Result<()> { Err(mlua::Error::runtime(CANCEL_MARKER)) })?)?;
    aegisub.set("gettext", lua.create_function(|_, s: String| Ok(s))?)?;
    aegisub.set("lua_automation_version", 4)?;
    {
        let inn = inner.clone();
        aegisub.set(
            "set_undo_point",
            lua.create_function(move |_, d: Option<String>| {
                inn.borrow_mut().undo_desc = d;
                Ok(())
            })?,
        )?;
    }

    // ---- ilerleme
    let progress = lua.create_table()?;
    {
        let inn = inner.clone();
        progress.set("set", lua.create_function(move |_, f: f64| { inn.borrow_mut().host.progress(f / 100.0); Ok(()) })?)?;
        let inn = inner.clone();
        progress.set("task", lua.create_function(move |_, t: Variadic<Value>| { if let Some(Value::String(s)) = t.first() { inn.borrow_mut().host.progress_task(&s.to_string_lossy()); } Ok(()) })?)?;
        let inn = inner.clone();
        progress.set("title", lua.create_function(move |_, t: Variadic<Value>| { if let Some(Value::String(s)) = t.first() { inn.borrow_mut().host.progress_title(&s.to_string_lossy()); } Ok(()) })?)?;
        let inn = inner.clone();
        progress.set("is_cancelled", lua.create_function(move |_, ()| Ok(inn.borrow_mut().host.is_cancelled()))?)?;
    }
    aegisub.set("progress", progress)?;

    // ---- iletişim kutuları
    let dialog = lua.create_table()?;
    {
        let inn = inner.clone();
        dialog.set(
            "display",
            lua.create_function(move |lua, (d, buttons, ids): (Table, Option<Table>, Option<Table>)| {
                let spec = DialogSpec::from_lua(&d, buttons.as_ref(), ids.as_ref())?;
                let r = inn.borrow_mut().host.dialog(&spec);
                let res = lua.create_table()?;
                for (k, v) in &r.values {
                    // Aegisub renk denetimleri değeri her zaman "#RRGGBB" ("coloralpha": "#RRGGBBAA") döndürür
                    let class = spec.controls.iter().find(|c| &c.name == k).map(|c| c.class.to_ascii_lowercase());
                    if let (Some(cl @ ("color" | "coloralpha")), DV::Str(sv)) = (class.as_deref(), v) {
                        if let Some(c) = osisub_core::color::Color::parse_dialog(sv) {
                            res.set(k.as_str(), c.to_hex(cl == "coloralpha"))?;
                            continue;
                        }
                    }
                    // intedit tam sayı döner (Aegisub lua_pushinteger), sınırlar içinde
                    if let (Some("intedit"), DV::Num(n)) = (class.as_deref(), v) {
                        let c = spec.controls.iter().find(|c| &c.name == k);
                        let mut n = n.trunc();
                        if let Some(lo) = c.and_then(|c| c.min) { n = n.max(lo); }
                        if let Some(hi) = c.and_then(|c| c.max) { n = n.min(hi); }
                        res.set(k.as_str(), n as i64)?;
                        continue;
                    }
                    res.set(k.as_str(), dv_to_lua(lua, v)?)?;
                }
                let b = match r.button {
                    None => Value::Boolean(false),
                    Some(label) => {
                        if spec.buttons.is_empty() {
                            Value::Boolean(true)
                        } else {
                            Value::String(lua.create_string(&label)?)
                        }
                    }
                };
                Ok((b, res))
            })?,
        )?;
        let inn = inner.clone();
        dialog.set(
            "open",
            lua.create_function(move |lua, (title, file, dir, wild, multiple, _must_exist): (Option<String>, Option<String>, Option<String>, Option<String>, Option<bool>, Option<bool>)| {
                let multiple = multiple.unwrap_or(false);
                let r = inn.borrow_mut().host.open_dialog(&title.unwrap_or_default(), &dir.unwrap_or_default(), &file.unwrap_or_default(), &wild.unwrap_or_default(), multiple);
                match r {
                    None => Ok(Value::Nil),
                    Some(v) if multiple => {
                        let t = lua.create_table()?;
                        for (i, s) in v.iter().enumerate() {
                            t.set(i + 1, s.as_str())?;
                        }
                        Ok(Value::Table(t))
                    }
                    Some(v) => Ok(v.first().map(|s| lua.create_string(s).map(Value::String)).transpose()?.unwrap_or(Value::Nil)),
                }
            })?,
        )?;
        let inn = inner.clone();
        dialog.set(
            "save",
            lua.create_function(move |_, (title, file, dir, wild, _overwrite): (Option<String>, Option<String>, Option<String>, Option<String>, Option<bool>)| {
                Ok(inn.borrow_mut().host.save_dialog(&title.unwrap_or_default(), &dir.unwrap_or_default(), &file.unwrap_or_default(), &wild.unwrap_or_default()))
            })?,
        )?;
    }
    aegisub.set("dialog", dialog)?;

    // ---- aegisub.parse_karaoke_data(line): Aegisub ile aynı biçim; [0] = ilk \k öncesi (boş olabilir)
    aegisub.set(
        "parse_karaoke_data",
        lua.create_function(|lua, line: Table| {
            let text: String = line.get::<Option<String>>("text")?.unwrap_or_default();
            let out = lua.create_table()?;
            let mk = |dur: i64, start: i64, tag: &str, t: &str, stripped: &str| -> Result<Table> {
                let s = lua.create_table()?;
                s.set("duration", dur)?;
                s.set("start_time", start)?;
                s.set("end_time", start + dur)?;
                s.set("tag", tag)?;
                s.set("text", t)?;
                s.set("text_stripped", stripped)?;
                Ok(s)
            };
            // Aegisub 3: [0] her zaman boş (2.x uyumu); ilk \k öncesi metin [1] olur (auto4_lua.cpp)
            out.raw_set(0, mk(0, 0, "", "", "")?)?;
            for (i, sy) in osisub_core::karaoke::parse_aegisub(&text).iter().enumerate() {
                out.raw_set(i as i64 + 1, mk(sy.dur_ms, sy.start_ms, &sy.tag, &sy.text, &sy.stripped)?)?;
            }
            Ok(out)
        })?,
    )?;

    // ---- ölçüler, kareler, proje
    {
        let inn = inner.clone();
        aegisub.set(
            "text_extents",
            lua.create_function(move |_, (style, text): (Table, String)| {
                let m = style_metrics(&style);
                let mut i = inn.borrow_mut();
                let r = match i.host.text_extents(&m, &text) {
                    Some(r) => r,
                    None => match i.fonts.extents(&m, &text) {
                        Some(r) => r,
                        None => approx_extents(&m, &text),
                    },
                };
                Ok(r)
            })?,
        )?;
        let inn = inner.clone();
        aegisub.set(
            "frame_from_ms",
            lua.create_function(move |_, ms: f64| Ok(inn.borrow().host.framerate().map(|f| f.frame_at_start(ms as i64))))?,
        )?;
        let inn = inner.clone();
        aegisub.set(
            "ms_from_frame",
            lua.create_function(move |_, fr: f64| Ok(inn.borrow().host.framerate().map(|f| f.time_at_start(fr as i64))))?,
        )?;
        let inn = inner.clone();
        aegisub.set(
            "video_size",
            lua.create_function(move |_, ()| {
                Ok(match inn.borrow().host.video_size() {
                    Some((w, h)) => MultiValue::from_iter([Value::Integer(w as i64), Value::Integer(h as i64), Value::Number(w as f64 / h as f64), Value::Integer(0)]),
                    None => MultiValue::from_iter([Value::Nil]),
                })
            })?,
        )?;
        let inn = inner.clone();
        let sc = script.clone();
        aegisub.set(
            "get_frame",
            lua.create_function(move |_, (frame, with_subs): (i64, Option<bool>)| {
                let ass = if with_subs.unwrap_or(false) { Some(sc.borrow().to_ass_string()) } else { None };
                let r = inn.borrow_mut().host.get_frame(frame, ass);
                Ok(r.map(|(w, h, rgba)| VideoFrame { w, h, rgba }))
            })?,
        )?;
        let inn = inner.clone();
        aegisub.set(
            "keyframes",
            lua.create_function(move |lua, ()| {
                let t = lua.create_table()?;
                for (i, k) in inn.borrow().host.keyframes().into_iter().enumerate() {
                    t.set(i + 1, k)?;
                }
                Ok(t)
            })?,
        )?;
        let inn = inner.clone();
        aegisub.set(
            "file_name",
            lua.create_function(move |_, ()| Ok(inn.borrow().props.get("script_file").map(|p| std::path::Path::new(p).file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default())))?,
        )?;
        let inn = inner.clone();
        aegisub.set(
            "get_audio_selection",
            lua.create_function(move |_, ()| {
                Ok(match inn.borrow().host.audio_selection() {
                    Some((a, b)) => MultiValue::from_iter([Value::Integer(a), Value::Integer(b)]),
                    None => MultiValue::new(),
                })
            })?,
        )?;
        // aegisub.gui (Aegisub 3.3+/arch1t3cht): metin kutusundaki imleç/seçim, 1 tabanlı bayt konumları.
        // Başlangıç değerleri ana makineden (`text_sel_start/end` özellikleri), ayarlananlar makro bitince uygulanır.
        let gui = lua.create_table()?;
        let cur_sel = |i: &crate::engine::Inner| -> (i64, i64) {
            i.text_sel_staged.unwrap_or_else(|| {
                let g = |k: &str| i.props.get(k).and_then(|v| v.parse::<i64>().ok()).unwrap_or(0);
                (g("text_sel_start"), g("text_sel_end"))
            })
        };
        let inn = inner.clone();
        gui.set("get_cursor", lua.create_function(move |_, ()| Ok(cur_sel(&inn.borrow()).1 + 1))?)?;
        let inn = inner.clone();
        gui.set(
            "get_selection",
            lua.create_function(move |_, ()| {
                let (a, b) = cur_sel(&inn.borrow());
                Ok((a.min(b) + 1, a.max(b) + 1))
            })?,
        )?;
        let inn = inner.clone();
        gui.set(
            "set_cursor",
            lua.create_function(move |_, p: f64| {
                let p = p as i64 - 1;
                inn.borrow_mut().text_sel_staged = Some((p, p));
                Ok(())
            })?,
        )?;
        let inn = inner.clone();
        gui.set(
            "set_selection",
            lua.create_function(move |_, (a, b): (f64, f64)| {
                inn.borrow_mut().text_sel_staged = Some((a as i64 - 1, b as i64 - 1));
                Ok(())
            })?,
        )?;
        let inn = inner.clone();
        gui.set("is_modified", lua.create_function(move |_, ()| Ok(inn.borrow().props.get("is_modified").map_or(false, |v| v == "1")))?)?;
        aegisub.set("gui", gui)?;
        let inn = inner.clone();
        aegisub.set(
            "set_status_text",
            lua.create_function(move |_, t: String| {
                inn.borrow_mut().status_text = Some(t);
                Ok(())
            })?,
        )?;
        let inn = inner.clone();
        aegisub.set(
            "project_properties",
            lua.create_function(move |lua, ()| {
                let t = lua.create_table()?;
                let i = inn.borrow();
                for k in ["video_file", "audio_file", "keyframes_file", "timecodes_file", "automation_scripts", "export_encoding"] {
                    t.set(k, i.props.get(k).cloned().unwrap_or_default())?;
                }
                t.set("video_position", i.props.get("video_position").and_then(|v| v.parse::<i64>().ok()).unwrap_or(0))?;
                t.set("video_zoom", 1.0)?;
                t.set("ar_mode", 0)?;
                t.set("ar_value", 0.0)?;
                t.set("export_filters", i.props.get("export_filters").cloned().unwrap_or_default())?;
                t.set("style_storage", i.props.get("style_storage").cloned().unwrap_or_default())?;
                t.set("scroll_position", i.props.get("scroll_position").and_then(|v| v.parse::<i64>().ok()).unwrap_or(0))?;
                t.set("active_row", i.props.get("active_row").and_then(|v| v.parse::<i64>().ok()).unwrap_or(0))?;
                t.set("script_file", i.props.get("script_file").cloned().unwrap_or_default())?;
                Ok(t)
            })?,
        )?;
        let inn = inner.clone();
        aegisub.set(
            "decode_path",
            lua.create_function(move |_, p: String| {
                let i = inn.borrow();
                let get = |k: &str, d: &str| i.props.get(k).cloned().unwrap_or_else(|| d.to_string());
                let user_dir = osisub_core::paths::config_dir().map(|d| d.to_string_lossy().to_string()).unwrap_or_else(|| ".".into());
                let repl = |tag: &str| -> Option<String> {
                    Some(match tag {
                        "?script" => get("script_dir", "."),
                        "?video" => PathBuf::from(get("video_file", "")).parent().map(|x| x.to_string_lossy().to_string()).unwrap_or_default(),
                        "?audio" => PathBuf::from(get("audio_file", "")).parent().map(|x| x.to_string_lossy().to_string()).unwrap_or_default(),
                        // Aegisub ?data: uygulama verisi (automation/ klasörünü içerir) → osisub ayar klasörü
                        "?data" => get("data_dir", &user_dir),
                        "?user" => get("user_dir", &user_dir),
                        "?local" => get("user_dir", &user_dir),
                        "?temp" => std::env::temp_dir().to_string_lossy().to_string(),
                        "?dictionary" => format!("{}/dictionaries", get("data_dir", &user_dir)),
                        _ => return None,
                    })
                };
                for tag in ["?script", "?video", "?audio", "?data", "?user", "?local", "?temp", "?dictionary"] {
                    if let Some(rest) = p.strip_prefix(tag) {
                        if let Some(base) = repl(tag).filter(|b| !b.is_empty()) {
                            return Ok(format!("{}{}", base, rest));
                        }
                    }
                }
                Ok(p)
            })?,
        )?;
    }

    // ---- re
    let re = lua.create_table()?;
    install_re(lua, &re)?;
    lua.load(RE_EXTRA).set_name("=re").call::<()>(re.clone())?;
    aegisub.set("__re", re)?;

    // ---- unicode
    let uni = lua.create_table()?;
    install_unicode(lua, &uni)?;
    aegisub.set("__unicode", uni)?;

    // ---- clipboard
    let clip = lua.create_table()?;
    {
        let inn = inner.clone();
        clip.set(
            "get",
            lua.create_function(move |_, ()| {
                let mut i = inn.borrow_mut();
                let v = i.host.clipboard_get();
                Ok(v.unwrap_or_else(|| i.clipboard.clone()))
            })?,
        )?;
        let inn = inner.clone();
        clip.set(
            "set",
            lua.create_function(move |_, s: String| {
                let mut i = inn.borrow_mut();
                if !i.host.clipboard_set(&s) {
                    i.clipboard = s;
                }
                Ok(true)
            })?,
        )?;
    }
    aegisub.set("__clipboard", clip)?;

    // ---- lfs
    let lfs = lua.create_table()?;
    install_lfs(lua, &lfs)?;
    aegisub.set("__lfs", lfs)?;

    g.set("aegisub", aegisub)?;
    Ok(())
}

// ------------------------------------------------------------------ re

/// Boost (Aegisub `re`) sözdizimi farkları: `\h`/`\H` boost'ta yatay boşluk, fancy-regex'te
/// onaltılık rakam — yatay boşluk sınıfına çevrilir.
fn boost_pattern(pat: &str) -> String {
    let mut out = String::with_capacity(pat.len());
    let mut it = pat.chars();
    while let Some(c) = it.next() {
        if c == '\\' {
            match it.next() {
                Some('h') => out.push_str("[\\t\\p{Zs}]"),
                Some('H') => out.push_str("[^\\t\\p{Zs}]"),
                Some(n) => { out.push('\\'); out.push(n); }
                None => out.push('\\'),
            }
        } else {
            out.push(c);
        }
    }
    out
}

fn compile(pat: &str, icase: bool) -> Result<Regex> {
    if pat.is_empty() {
        return Err(mlua::Error::runtime("Regular expression must not be empty"));
    }
    // harf duyarsızlığı satır içi (?i): fancy-regex'te bakış ifadeli desende yapıcı bayrağı işlemiyor
    let p = boost_pattern(pat);
    let p = if icase { format!("(?i){p}") } else { p };
    RegexBuilder::new(&p).build().map_err(|e| mlua::Error::runtime(format!("re: geçersiz ifade: {}", e)))
}

fn match_table(lua: &Lua, s: &str, start: usize, end: usize) -> Result<Table> {
    let t = lua.create_table()?;
    t.set("str", &s[start..end])?;
    t.set("first", start + 1)?;
    t.set("last", end)?;
    Ok(t)
}

fn find_all(lua: &Lua, re: &Regex, s: &str) -> Result<Value> {
    let out = lua.create_table()?;
    let mut n = 0;
    for m in re.find_iter(s) {
        let m = m.map_err(mlua::Error::external)?;
        n += 1;
        out.set(n, match_table(lua, s, m.start(), m.end())?)?;
    }
    Ok(if n == 0 { Value::Nil } else { Value::Table(out) })
}

fn captures_table(lua: &Lua, s: &str, c: &fancy_regex::Captures) -> Result<Table> {
    let out = lua.create_table()?;
    for i in 0..c.len() {
        match c.get(i) {
            Some(m) => out.set(i + 1, match_table(lua, s, m.start(), m.end())?)?,
            None => out.set(i + 1, lua.create_table_from([("str", ""), ])?)?,
        }
    }
    Ok(out)
}

fn match_first(lua: &Lua, re: &Regex, s: &str, start: usize) -> Result<Value> {
    let mut pos = start.saturating_sub(1).min(s.len());
    while !s.is_char_boundary(pos) { pos += 1; }
    let Some(c) = re.captures_from_pos(s, pos).map_err(mlua::Error::external)? else { return Ok(Value::Nil) };
    Ok(Value::Table(captures_table(lua, s, &c)?))
}

/// Aegisub `gmatch`: `start` (1 tabanlı bayt) konumundan itibaren her eşleşmenin `match` tablosu.
fn match_all(lua: &Lua, re: &Regex, s: &str, start: usize) -> Result<Table> {
    let out = lua.create_table()?;
    let mut pos = start.saturating_sub(1).min(s.len());
    while !s.is_char_boundary(pos) { pos += 1; }
    let mut n = 0;
    while pos <= s.len() {
        let Some(c) = re.captures_from_pos(s, pos).map_err(mlua::Error::external)? else { break };
        let m = c.get(0).expect("0. grup");
        n += 1;
        out.set(n, captures_table(lua, s, &c)?)?;
        pos = if m.end() > m.start() { m.end() } else {
            // boş eşleşmede bir karakter ilerle
            match s[m.end()..].chars().next() { Some(ch) => m.end() + ch.len_utf8(), None => break }
        };
    }
    Ok(out)
}

fn convert_repl(r: &str) -> String {
    // \1 -> ${1}; $1 zaten desteklenir; "$$" korunur.
    let mut out = String::new();
    let b: Vec<char> = r.chars().collect();
    let mut i = 0;
    while i < b.len() {
        if b[i] == '\\' && i + 1 < b.len() && b[i + 1].is_ascii_digit() {
            out.push_str(&format!("${{{}}}", b[i + 1]));
            i += 2;
        } else if b[i] == '$' && i + 1 < b.len() && b[i + 1].is_ascii_digit() {
            out.push_str(&format!("${{{}}}", b[i + 1]));
            i += 2;
        } else {
            out.push(b[i]);
            i += 1;
        }
    }
    out
}

fn sub_impl(re: &Regex, s: &str, repl: Value, count: Option<i64>) -> Result<String> {
    let limit = count.filter(|c| *c > 0).map(|c| c as usize).unwrap_or(0);
    match repl {
        Value::Function(f) => {
            // Aegisub re.moon: grupsuz desende işlev tüm eşleşmeyle, gruplu desende her yakalama grubu
            // için (str, first, last) ile çağrılır; dize dönmezse özgün metin kalır.
            let call = |text: &str, a: usize, b: usize| -> Result<Option<String>> {
                match f.call::<Value>((text, a + 1, b))? {
                    Value::String(r) => Ok(Some(r.to_str()?.to_string())),
                    _ => Ok(None),
                }
            };
            let mut out = String::new();
            let mut last = 0;
            let mut n = 0;
            let mut pos = 0;
            while pos <= s.len() {
                let Some(c) = re.captures_from_pos(s, pos).map_err(mlua::Error::external)? else { break };
                let m = c.get(0).expect("0. grup");
                if limit != 0 && n >= limit {
                    break;
                }
                out.push_str(&s[last..m.start()]);
                if c.len() <= 1 {
                    out.push_str(&call(m.as_str(), m.start(), m.end())?.unwrap_or_else(|| m.as_str().to_string()));
                } else {
                    let mut cur = m.start();
                    for g in 1..c.len() {
                        let Some(cm) = c.get(g) else { continue };
                        if cm.start() < cur { continue; } // iç içe grup: dış grup zaten yazıldı
                        out.push_str(&s[cur..cm.start()]);
                        out.push_str(&call(cm.as_str(), cm.start(), cm.end())?.unwrap_or_else(|| cm.as_str().to_string()));
                        cur = cm.end();
                    }
                    out.push_str(&s[cur..m.end()]);
                }
                last = m.end();
                n += 1;
                pos = if m.end() > m.start() { m.end() } else {
                    match s[m.end()..].chars().next() { Some(ch) => m.end() + ch.len_utf8(), None => break }
                };
            }
            out.push_str(&s[last.min(s.len())..]);
            Ok(out)
        }
        Value::String(r) => {
            let r = convert_repl(&r.to_str()?);
            Ok(if limit == 0 { re.replace_all(s, r.as_str()).into_owned() } else { re.replacen(s, limit, r.as_str()).into_owned() })
        }
        _ => Err(mlua::Error::runtime("re.sub: değiştirme dizesi gerekli")),
    }
}

/// Aegisub re.lua'nın yineleyici/bölme yöntemleri (derlenmiş nesneye ve modüle).
const RE_EXTRA: &str = r#"
local re = ...
local function add(o)
  function o.gfind(self, s)
    local all, i = self:find(s) or {}, 0
    return function() i = i + 1; local m = all[i]; if m then return m.str, m.first, m.last end end
  end
  function o.gmatch(self, s, start)
    local all, i = self:_match_all(s, start), 0
    return function() i = i + 1; return all[i] end
  end
  function o.split(self, s, skip_empty, max_split)
    local out, pos, n = {}, 1, 0
    for _, m in ipairs(self:find(s) or {}) do
      if max_split and max_split > 0 and n >= max_split then break end
      if m.last >= m.first then
        local piece = s:sub(pos, m.first - 1)
        if not (skip_empty and piece == "") then out[#out + 1] = piece end
        pos, n = m.last + 1, n + 1
      end
    end
    local piece = s:sub(pos)
    if not (skip_empty and piece == "") then out[#out + 1] = piece end
    return out
  end
  function o.gsplit(self, s, skip_empty, max_split)
    local parts, i = self:split(s, skip_empty, max_split), 0
    return function() i = i + 1; return parts[i] end
  end
  return o
end
local raw = re.compile
re.compile = function(p, ...) return add(raw(p, ...)) end
re.split = function(s, p, skip_empty, max_split, ...) return re.compile(p, ...):split(s, skip_empty, max_split) end
re.gsplit = function(s, p, skip_empty, max_split, ...) return re.compile(p, ...):gsplit(s, skip_empty, max_split) end
re.gmatch = function(s, p, ...)
  local start, flags = nil, {}
  for _, a in ipairs({...}) do if type(a) == "number" then start = a else flags[#flags + 1] = a end end
  return re.compile(p, unpack(flags)):gmatch(s, start)
end
re.gfind = function(s, p, ...) return re.compile(p, ...):gfind(s) end
"#;

/// Bağımsız değişkenlerden `re.ICASE` gibi bayrakları ayırır (Aegisub: `agi_re_flag` cdata;
/// burada `{__re_flag = n}` tablosu). Kalanlar sırayla döner (başlangıç, en çok değiştirme…).
fn split_re_args(v: &Variadic<Value>) -> (i64, Vec<Value>) {
    let mut flags = 0;
    let mut rest = vec![];
    for x in v.iter() {
        match x {
            Value::Table(t) => match t.raw_get::<Option<i64>>("__re_flag") {
                Ok(Some(f)) => flags |= f,
                _ => rest.push(x.clone()),
            },
            Value::Nil => {}
            _ => rest.push(x.clone()),
        }
    }
    (flags, rest)
}

fn num_arg(v: Option<&Value>) -> Option<i64> {
    match v {
        Some(Value::Integer(i)) => Some(*i),
        Some(Value::Number(n)) => Some(*n as i64),
        _ => None,
    }
}

fn install_re(lua: &Lua, re: &Table) -> Result<()> {
    for (name, v) in [("ICASE", 1), ("NOSUB", 2), ("BASIC", 4), ("NEWLINE", 8), ("COLLATE", 16), ("EXTENDED", 32), ("NOSUBS", 2), ("NO_MOD_M", 64)] {
        let t = lua.create_table()?;
        t.set("__re_flag", v)?;
        re.set(name, t)?;
    }
    re.set("find", lua.create_function(move |lua, (s, p, f): (String, String, Variadic<Value>)| find_all(lua, &compile(&p, split_re_args(&f).0 & 1 != 0)?, &s))?)?;
    re.set("match", lua.create_function(move |lua, (s, p, f): (String, String, Variadic<Value>)| {
        let (fl, rest) = split_re_args(&f);
        match_first(lua, &compile(&p, fl & 1 != 0)?, &s, num_arg(rest.first()).unwrap_or(1).max(1) as usize)
    })?)?;
    let sub = lua.create_function(move |_, (s, p, r, rest): (String, String, Value, Variadic<Value>)| {
        let (fl, rest) = split_re_args(&rest);
        sub_impl(&compile(&p, fl & 1 != 0)?, &s, r, num_arg(rest.first()))
    })?;
    re.set("sub", sub.clone())?;
    re.set("gsub", sub)?;
    re.set(
        "compile",
        lua.create_function(move |lua, (p, f): (String, Variadic<Value>)| {
            let rx = Rc::new(compile(&p, split_re_args(&f).0 & 1 != 0)?);
            let t = lua.create_table()?;
            let r = rx.clone();
            t.set("find", lua.create_function(move |lua, (_s, s): (Value, String)| find_all(lua, &r, &s))?)?;
            let r = rx.clone();
            t.set("match", lua.create_function(move |lua, (_s, s, start): (Value, String, Option<i64>)| match_first(lua, &r, &s, start.unwrap_or(1).max(1) as usize))?)?;
            let r = rx.clone();
            let sub = lua.create_function(move |_, (_s, s, rep, count): (Value, String, Value, Option<i64>)| sub_impl(&r, &s, rep, count))?;
            t.set("sub", sub.clone())?;
            t.set("gsub", sub)?;
            let r = rx.clone();
            t.set("_match_all", lua.create_function(move |lua, (_s, s, start): (Value, String, Option<usize>)| match_all(lua, &r, &s, start.unwrap_or(1)))?)?;
            Ok(t)
        })?,
    )?;
    Ok(())
}

// ------------------------------------------------------------------ unicode

/// Aegisub unicode.moon'dan birebir: bayt tabanlı (geçersiz UTF-8'de hata vermez).
const UNICODE_IMPL: &str = r#"
local unicode = ...
function unicode.charwidth(s, i)
  local b = s:byte(i or 1)
  if not b then return 1 elseif b < 128 then return 1 elseif b < 224 then return 2 elseif b < 240 then return 3 else return 4 end
end
function unicode.chars(s)
  local curchar, i = 0, 1
  return function()
    if i > s:len() then return end
    local j = i
    curchar = curchar + 1
    i = i + unicode.charwidth(s, i)
    return s:sub(j, i - 1), curchar
  end
end
function unicode.len(s)
  local n = 0
  for _ in unicode.chars(s) do n = n + 1 end
  return n
end
function unicode.codepoint(s)
  local b = s:byte(1)
  if not b then return nil end
  if b < 128 then return b end
  local res, w
  if b < 224 then res, w = b - 192, 2 elseif b < 240 then res, w = b - 224, 3 else res, w = b - 240, 4 end
  for i = 2, w do res = res * 64 + (s:byte(i) or 128) - 128 end
  return res
end
"#;

fn install_unicode(lua: &Lua, u: &Table) -> Result<()> {
    // büyük/küçük harf: geçersiz UTF-8 bayrakları hata yerine U+FFFD olur
    let conv = |f: fn(&str) -> String| move |_: &Lua, s: mlua::String| Ok(f(&s.to_string_lossy()));
    u.set("to_upper", lua.create_function(conv(|s| s.to_uppercase()))?)?;
    u.set("to_lower", lua.create_function(conv(|s| s.to_lowercase()))?)?;
    u.set("to_fold", lua.create_function(conv(|s| s.to_lowercase()))?)?;
    // Aegisub unicode.lua adları
    u.set("to_upper_case", lua.create_function(conv(|s| s.to_uppercase()))?)?;
    u.set("to_lower_case", lua.create_function(conv(|s| s.to_lowercase()))?)?;
    u.set("to_fold_case", lua.create_function(conv(|s| s.to_lowercase()))?)?;
    u.set("tr_upper", lua.create_function(conv(osisub_core::qc::tr_upper))?)?;
    u.set("tr_lower", lua.create_function(conv(osisub_core::qc::tr_lower))?)?;
    lua.load(UNICODE_IMPL).set_name("osisub/unicode").call::<()>(u.clone())?;
    Ok(())
}

// ------------------------------------------------------------------ lfs

fn install_lfs(lua: &Lua, l: &Table) -> Result<()> {
    l.set(
        "attributes",
        lua.create_function(|lua, (p, key): (String, Option<String>)| {
            let Ok(md) = std::fs::metadata(&p) else { return Ok((Value::Nil, Some("cannot obtain information".to_string()))) };
            let t = lua.create_table()?;
            t.set("mode", if md.is_dir() { "directory" } else { "file" })?;
            t.set("size", md.len())?;
            let mt = md.modified().ok().and_then(|m| m.duration_since(std::time::UNIX_EPOCH).ok()).map(|d| d.as_secs()).unwrap_or(0);
            t.set("modification", mt)?;
            if let Some(k) = key {
                return Ok((t.get::<Value>(k)?, None));
            }
            Ok((Value::Table(t), None))
        })?,
    )?;
    l.set(
        "dir",
        lua.create_function(|lua, p: String| {
            let mut names: Vec<String> = vec![".".into(), "..".into()];
            if let Ok(rd) = std::fs::read_dir(&p) {
                names.extend(rd.filter_map(|e| e.ok()).map(|e| e.file_name().to_string_lossy().to_string()));
            }
            let idx = Rc::new(RefCell::new(0usize));
            lua.create_function(move |_, ()| {
                let mut i = idx.borrow_mut();
                let r = names.get(*i).cloned();
                *i += 1;
                Ok(r)
            })
        })?,
    )?;
    l.set("mkdir", lua.create_function(|_, p: String| Ok(std::fs::create_dir(&p).is_ok()))?)?;
    l.set("rmdir", lua.create_function(|_, p: String| Ok(std::fs::remove_dir(&p).is_ok()))?)?;
    l.set("currentdir", lua.create_function(|_, ()| Ok(std::env::current_dir().ok().map(|p| p.to_string_lossy().to_string())))?)?;
    Ok(())
}

/// `aegisub.get_frame` sonucu (arch1t3cht Aegisub API'si): `frame:width()`, `frame:height()`,
/// `frame:getPixel(x, y)` (0xRRGGBB tamsayı), `frame:getPixelFormatted(x, y)` ("&HBBGGRR&").
pub struct VideoFrame {
    pub w: u32,
    pub h: u32,
    pub rgba: Vec<u8>,
}

impl VideoFrame {
    fn px(&self, x: i64, y: i64) -> Option<(u8, u8, u8)> {
        if x < 0 || y < 0 || x >= self.w as i64 || y >= self.h as i64 {
            return None;
        }
        let i = ((y as usize) * self.w as usize + x as usize) * 4;
        Some((self.rgba[i], self.rgba[i + 1], self.rgba[i + 2]))
    }
}

impl mlua::UserData for VideoFrame {
    fn add_methods<M: mlua::UserDataMethods<Self>>(m: &mut M) {
        m.add_method("width", |_, f, ()| Ok(f.w));
        m.add_method("height", |_, f, ()| Ok(f.h));
        m.add_method("getPixel", |_, f, (x, y): (i64, i64)| Ok(f.px(x, y).map(|(r, g, b)| ((r as i64) << 16) | ((g as i64) << 8) | b as i64)));
        m.add_method("getPixelFormatted", |_, f, (x, y): (i64, i64)| Ok(f.px(x, y).map(|(r, g, b)| format!("&H{:02X}{:02X}{:02X}&", b, g, r))));
    }
}
