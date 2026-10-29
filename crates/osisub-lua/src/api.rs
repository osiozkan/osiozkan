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
const UNICODE_LUA: &str = "unicode = aegisub.__unicode\nreturn unicode\n";

fn embedded(name: &str) -> Option<&'static str> {
    match name.rsplit(['/', '\\']).next().unwrap_or(name) {
        "utils.lua" => Some(UTILS),
        "karaskel.lua" => Some(KARASKEL),
        "unicode.lua" => Some(UNICODE_LUA),
        "utf8.lua" => Some(UTF8_LUA),
        "utils-auto4.lua" => Some(UTILS_AUTO4),
        "karaskel-auto4.lua" => Some(KARASKEL_AUTO4),
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
        w += em * fs * if st.bold { 1.07 } else { 1.0 } * st.scale_x / 100.0 + st.spacing;
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
        let inn = inner.clone();
        native.set(
            "log",
            lua.create_function(move |_, (level, msg): (i64, String)| {
                let mut i = inn.borrow_mut();
                i.host.log(level, &msg);
                i.log.push_str(&msg);
                Ok(())
            })?,
        )?;
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
            lua.create_function(move |_, ms: f64| Ok(inn.borrow().host.fps().map(|f| osisub_core::time::frame_from_ms(ms as i64, f))))?,
        )?;
        let inn = inner.clone();
        aegisub.set(
            "ms_from_frame",
            lua.create_function(move |_, fr: f64| Ok(inn.borrow().host.fps().map(|f| osisub_core::time::ms_from_frame(fr as i64, f))))?,
        )?;
        let inn = inner.clone();
        aegisub.set(
            "video_size",
            lua.create_function(move |_, ()| {
                Ok(match inn.borrow().host.video_size() {
                    Some((w, h)) => MultiValue::from_iter([Value::Integer(w as i64), Value::Integer(h as i64), Value::Number(w as f64 / h as f64), Value::Integer(0)]),
                    None => MultiValue::new(),
                })
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
        aegisub.set("get_cursor", lua.create_function(|_, ()| Ok(MultiValue::new()))?)?;
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
                        "?data" => get("data_dir", "."),
                        "?user" => get("user_dir", &user_dir),
                        "?local" => get("user_dir", &user_dir),
                        "?temp" => std::env::temp_dir().to_string_lossy().to_string(),
                        "?dictionary" => get("data_dir", "."),
                        _ => return None,
                    })
                };
                for tag in ["?script", "?video", "?audio", "?data", "?user", "?local", "?temp", "?dictionary"] {
                    if let Some(rest) = p.strip_prefix(tag) {
                        if let Some(base) = repl(tag) {
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

fn compile(pat: &str, icase: bool) -> Result<Regex> {
    RegexBuilder::new(pat).case_insensitive(icase).build().map_err(|e| mlua::Error::runtime(format!("re: geçersiz ifade: {}", e)))
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

fn match_first(lua: &Lua, re: &Regex, s: &str) -> Result<Value> {
    let Some(c) = re.captures(s).map_err(mlua::Error::external)? else { return Ok(Value::Nil) };
    let out = lua.create_table()?;
    for i in 0..c.len() {
        match c.get(i) {
            Some(m) => out.set(i + 1, match_table(lua, s, m.start(), m.end())?)?,
            None => out.set(i + 1, lua.create_table_from([("str", ""), ])?)?,
        }
    }
    Ok(Value::Table(out))
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
            let mut out = String::new();
            let mut last = 0;
            let mut n = 0;
            for m in re.find_iter(s) {
                let m = m.map_err(mlua::Error::external)?;
                if limit != 0 && n >= limit {
                    break;
                }
                out.push_str(&s[last..m.start()]);
                let r: String = f.call(m.as_str())?;
                out.push_str(&r);
                last = m.end();
                n += 1;
            }
            out.push_str(&s[last..]);
            Ok(out)
        }
        Value::String(r) => {
            let r = convert_repl(&r.to_str()?);
            Ok(if limit == 0 { re.replace_all(s, r.as_str()).into_owned() } else { re.replacen(s, limit, r.as_str()).into_owned() })
        }
        _ => Err(mlua::Error::runtime("re.sub: değiştirme dizesi gerekli")),
    }
}

fn install_re(lua: &Lua, re: &Table) -> Result<()> {
    re.set("ICASE", 1)?;
    re.set("NOSUB", 2)?;
    re.set("BASIC", 4)?;
    re.set("NEWLINE", 8)?;
    let flags = |v: Variadic<Value>| v.iter().fold(0i64, |a, x| a | match x { Value::Integer(i) => *i, Value::Number(n) => *n as i64, _ => 0 });
    re.set("find", lua.create_function(move |lua, (s, p, f): (String, String, Variadic<Value>)| find_all(lua, &compile(&p, flags(f) & 1 != 0)?, &s))?)?;
    re.set("match", lua.create_function(move |lua, (s, p, f): (String, String, Variadic<Value>)| match_first(lua, &compile(&p, flags(f) & 1 != 0)?, &s))?)?;
    let sub = lua.create_function(move |_, (s, p, r, rest): (String, String, Value, Variadic<Value>)| {
        let mut count = None;
        let mut fl = 0;
        for (i, v) in rest.iter().enumerate() {
            let n = match v { Value::Integer(i) => *i, Value::Number(n) => *n as i64, _ => 0 };
            if i == 0 { count = Some(n); } else { fl |= n; }
        }
        sub_impl(&compile(&p, fl & 1 != 0)?, &s, r, count)
    })?;
    re.set("sub", sub.clone())?;
    re.set("gsub", sub)?;
    re.set(
        "compile",
        lua.create_function(move |lua, (p, f): (String, Variadic<Value>)| {
            let rx = Rc::new(compile(&p, flags(f) & 1 != 0)?);
            let t = lua.create_table()?;
            let r = rx.clone();
            t.set("find", lua.create_function(move |lua, (_s, s): (Value, String)| find_all(lua, &r, &s))?)?;
            let r = rx.clone();
            t.set("match", lua.create_function(move |lua, (_s, s): (Value, String)| match_first(lua, &r, &s))?)?;
            let r = rx.clone();
            let sub = lua.create_function(move |_, (_s, s, rep, count): (Value, String, Value, Option<i64>)| sub_impl(&r, &s, rep, count))?;
            t.set("sub", sub.clone())?;
            t.set("gsub", sub)?;
            Ok(t)
        })?,
    )?;
    Ok(())
}

// ------------------------------------------------------------------ unicode

fn install_unicode(lua: &Lua, u: &Table) -> Result<()> {
    u.set("codepoint", lua.create_function(|_, s: String| Ok(s.chars().next().map(|c| c as u32).unwrap_or(0)))?)?;
    u.set("len", lua.create_function(|_, s: String| Ok(s.chars().count()))?)?;
    u.set("to_upper", lua.create_function(|_, s: String| Ok(s.to_uppercase()))?)?;
    u.set("to_lower", lua.create_function(|_, s: String| Ok(s.to_lowercase()))?)?;
    u.set("to_fold", lua.create_function(|_, s: String| Ok(s.to_lowercase()))?)?;
    u.set("tr_upper", lua.create_function(|_, s: String| Ok(osisub_core::qc::tr_upper(&s)))?)?;
    u.set("tr_lower", lua.create_function(|_, s: String| Ok(osisub_core::qc::tr_lower(&s)))?)?;
    u.set(
        "chars",
        lua.create_function(|lua, s: String| {
            let chars: Vec<String> = s.chars().map(|c| c.to_string()).collect();
            let idx = Rc::new(RefCell::new(0usize));
            lua.create_function(move |_, ()| {
                let mut i = idx.borrow_mut();
                let r = chars.get(*i).cloned();
                *i += 1;
                Ok(r)
            })
        })?,
    )?;
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
