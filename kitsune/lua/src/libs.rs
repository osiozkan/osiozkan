use super::*;
use mlua::{Lua, Table, Value};

const UTIL: &str = include_str!("../lib/util.lua");
const KARASKEL: &str = include_str!("../lib/karaskel.lua");

fn approx_extents(fontname: &str, size: f64, scale_x: f64, spacing: f64, text: &str) -> (f64, f64, f64, f64) {
    let _ = fontname;
    let mut w = 0.0;
    for c in text.chars() {
        let f = match c {
            ' ' => 0.28, 'i' | 'l' | 'j' | '.' | ',' | '\'' | '!' | '|' | ':' | ';' => 0.26,
            'm' | 'w' | 'M' | 'W' => 0.85, 'A'..='Z' => 0.66, '0'..='9' => 0.56,
            c if (c as u32) >= 0x2E80 => 1.0,
            _ => 0.52,
        };
        w += f * size * scale_x / 100.0 + spacing;
    }
    (w, size * 1.17, size * 0.22, 0.0)
}

pub fn install(lua: &Lua, script: &Rc<RefCell<Script>>, log: &Rc<RefCell<Vec<String>>>, opts: &HostOptions) -> mlua::Result<()> {
    let g = lua.globals();
    g.set("__macros", lua.create_table()?)?;
    let aeg = lua.create_table()?;

    // registration
    aeg.set("register_macro", lua.create_function(|lua, (name, desc, f, validate, active): (String, Option<String>, Function, Option<Function>, Option<Function>)| {
        let t = lua.create_table()?;
        t.set("name", name)?; t.set("description", desc.unwrap_or_default())?; t.set("fn", f)?;
        if let Some(v) = validate { t.set("validate", v)?; }
        if let Some(a) = active { t.set("is_active", a)?; }
        let reg: Table = lua.globals().get("__macros")?;
        reg.raw_set(reg.raw_len() + 1, t)?; Ok(())
    })?)?;
    aeg.set("register_filter", lua.create_function(|lua, (name, desc, _prio, f, _cfg): (String, Option<String>, Option<i64>, Function, Option<Function>)| {
        let t = lua.create_table()?;
        t.set("name", name)?; t.set("description", desc.unwrap_or_default())?; t.set("fn", f)?; t.set("filter", true)?;
        let reg: Table = lua.globals().get("__macros")?;
        reg.raw_set(reg.raw_len() + 1, t)?; Ok(())
    })?)?;

    // logging
    let l2 = log.clone();
    let out = lua.create_function(move |_, args: Variadic<Value>| {
        let mut it = args.into_iter().peekable();
        if let Some(Value::Integer(_)) | Some(Value::Number(_)) = it.peek() { it.next(); }
        let msg = match it.next() { Some(Value::String(s)) => s.to_str()?.to_string(), Some(Value::Integer(i)) => i.to_string(), Some(Value::Number(n)) => n.to_string(), Some(Value::Boolean(b)) => b.to_string(), _ => String::new() };
        let rest: Vec<String> = it.map(|v| match v { Value::String(s) => s.to_str().map(|x| x.to_string()).unwrap_or_default(), Value::Integer(i) => i.to_string(), Value::Number(n) => n.to_string(), _ => String::new() }).collect();
        // printf-style: replace %s/%d sequentially
        let mut res = String::new(); let mut ri = rest.into_iter(); let mut cs = msg.chars().peekable();
        while let Some(c) = cs.next() {
            if c == '%' { match cs.peek() { Some('%') => { cs.next(); res.push('%'); }
                Some(_) => { let mut spec = String::new(); while let Some(&n) = cs.peek() { cs.next(); spec.push(n); if n.is_ascii_alphabetic() { break; } } res.push_str(&ri.next().unwrap_or_default()); }
                None => res.push('%') } } else { res.push(c) }
        }
        l2.borrow_mut().push(res); Ok(())
    })?;
    let dbg = lua.create_table()?; dbg.set("out", out.clone())?;
    aeg.set("debug", dbg)?; aeg.set("log", out)?;
    aeg.set("cancel", lua.create_function(|_, ()| -> mlua::Result<()> { Err(mlua::Error::runtime(CANCEL)) })?)?;
    aeg.set("set_undo_point", lua.create_function(|_, _: Variadic<Value>| Ok(()))?)?;
    aeg.set("gettext", lua.create_function(|_, s: String| Ok(s))?)?;
    aeg.set("lua_supports", lua.create_function(|_, _: String| Ok(true))?)?;
    aeg.set("decode_path", lua.create_function(|lua, p: String| {
        let dir: Option<String> = lua.globals().get("__script_dir")?;
        Ok(p.replace("?script", dir.as_deref().unwrap_or(".")).replace("?data", ".").replace("?user", ".").replace("?temp", "/tmp"))
    })?)?;
    aeg.set("file_name", lua.create_function(|_, ()| Ok("untitled.ass".to_string()))?)?;
    let progress = lua.create_table()?;
    for n in ["set", "task", "title"] { progress.set(n, lua.create_function(|_, _: Variadic<Value>| Ok(()))?)?; }
    progress.set("is_cancelled", lua.create_function(|_, ()| Ok(false))?)?;
    aeg.set("progress", progress)?;

    // video / timing
    let fps = opts.fps;
    aeg.set("frame_from_ms", lua.create_function(move |_, ms: f64| Ok(fps.map(|f| (ms * f / 1000.0).floor() as i64)))?)?;
    aeg.set("ms_from_frame", lua.create_function(move |_, fr: f64| Ok(fps.map(|f| (fr * 1000.0 / f).round() as i64)))?)?;
    let vs = opts.video_size;
    aeg.set("video_size", lua.create_function(move |_, ()| Ok(match vs { Some((w, h)) => (Some(w), Some(h), Some(w as f64 / h as f64), Some(0)), None => (None, None, None, None) }))?)?;
    let vf = opts.video_file.clone();
    aeg.set("project_properties", lua.create_function(move |lua, ()| {
        let t = lua.create_table()?;
        if let Some(v) = &vf { t.set("video_file", v.as_str())?; }
        t.set("automation_scripts", "")?; Ok(t)
    })?)?;
    aeg.set("keyframes", lua.create_function(|lua, ()| Ok((lua.create_table()?, 0)))?)?;
    aeg.set("get_audio_selection", lua.create_function(|_, ()| Ok((0i64, 0i64)))?)?;
    aeg.set("text_extents", lua.create_function(|_, (style, text): (Table, String)| {
        let size = style.get::<Option<f64>>("fontsize")?.unwrap_or(48.0);
        let sx = style.get::<Option<f64>>("scale_x")?.unwrap_or(100.0);
        let sp = style.get::<Option<f64>>("spacing")?.unwrap_or(0.0);
        let name = style.get::<Option<String>>("fontname")?.unwrap_or_default();
        // text may contain no tags; Aegisub measures the raw string
        let (w, h, d, e) = approx_extents(&name, size, sx, sp, &text);
        Ok((w, h, d, e))
    })?)?;

    // dialogs (headless: defaults + overrides). A GUI host replaces this later.
    let ov = opts.dialog_overrides.clone();
    let dialog = lua.create_table()?;
    dialog.set("display", lua.create_function(move |lua, (cfg, buttons, ids): (Table, Option<Table>, Option<Table>)| {
        let res = lua.create_table()?;
        for c in cfg.sequence_values::<Table>() {
            let c = c?;
            let Some(name) = c.get::<Option<String>>("name")? else { continue };
            let class = c.get::<Option<String>>("class")?.unwrap_or_default();
            let val: Value = if let Some(o) = ov.get(&name) {
                match class.as_str() { "checkbox" => Value::Boolean(o == "true" || o == "1"),
                    "intedit" | "floatedit" | "scrollint" | "scrollfloat" => Value::Number(o.parse().unwrap_or(0.0)),
                    _ => Value::String(lua.create_string(o)?) }
            } else {
                let v: Value = c.get("value")?;
                if matches!(v, Value::Nil) {
                    match class.as_str() { "checkbox" => Value::Boolean(false), "intedit" | "floatedit" => c.get::<Option<f64>>("min")?.map(Value::Number).unwrap_or(Value::Number(0.0)),
                        "dropdown" => c.get::<Table>("items").ok().and_then(|t| t.get::<Value>(1).ok()).unwrap_or(Value::Nil),
                        "edit" | "textbox" => Value::String(lua.create_string("")?), _ => Value::Nil }
                } else { v }
            };
            res.set(name, val)?;
        }
        let btn: Value = match buttons {
            None => Value::Boolean(true),
            Some(b) => {
                let ok = ids.as_ref().and_then(|i| i.get::<Option<String>>("ok").ok().flatten());
                match ok { Some(s) => Value::String(lua.create_string(&s)?), None => b.get::<Value>(1)? }
            }
        };
        Ok((btn, res))
    })?)?;
    dialog.set("open", lua.create_function(|_, _: Variadic<Value>| Ok(Value::Nil))?)?;
    dialog.set("save", lua.create_function(|_, _: Variadic<Value>| Ok(Value::Nil))?)?;
    aeg.set("dialog", dialog)?;
    g.set("aegisub", aeg)?;

    // Lua 5.1 style helpers LuaJIT already has. `include` loads from script dir / include dirs.
    g.set("include", lua.load(r#"
        return function(name)
            local dir = __script_dir or "."
            for _, p in ipairs{dir.."/"..name, dir.."/include/"..name, "include/"..name, name} do
                local f = io.open(p, "r")
                if f then f:close(); return dofile(p) end
            end
            local mod = name:gsub("%.lua$", "")
            local m = require(mod)
            if _G[mod] == nil then _G[mod] = m end
            return m
        end"#).eval::<Function>()?)?;

    // unicode
    let uni = lua.create_table()?;
    uni.set("len", lua.create_function(|_, s: mlua::String| Ok(String::from_utf8_lossy(&s.as_bytes()).chars().count()))?)?;
    uni.set("charclass", lua.create_function(|_, (s, i): (String, usize)| {
        Ok(s.chars().nth(i.saturating_sub(1)).map(|c| if c.is_alphabetic() { 1 } else if c.is_whitespace() { 2 } else { 0 }).unwrap_or(0))
    })?)?;
    uni.set("codepoint", lua.create_function(|_, s: String| Ok(s.chars().next().map(|c| c as u32).unwrap_or(0)))?)?;
    uni.set("chars", lua.create_function(|lua, s: String| {
        let chars: Vec<String> = s.chars().map(String::from).collect();
        let mut i = 0usize;
        lua.create_function_mut(move |_, ()| { i += 1; Ok(chars.get(i - 1).cloned()) })
    })?)?;
    uni.set("lower", lua.create_function(|_, s: String| Ok(s.to_lowercase()))?)?;
    uni.set("upper", lua.create_function(|_, s: String| Ok(s.to_uppercase()))?)?;
    uni.set("to_lower", lua.create_function(|_, s: String| Ok(s.to_lowercase()))?)?;
    uni.set("to_upper", lua.create_function(|_, s: String| Ok(s.to_uppercase()))?)?;
    uni.set("to_fold", lua.create_function(|_, s: String| Ok(s.to_lowercase()))?)?;
    let package: Table = g.get("package")?;
    let loaded: Table = package.get("loaded")?;
    loaded.set("unicode", uni.clone())?;
    loaded.set("aegisub.unicode", uni)?;
    // utf8 module for Aegisub (it ships `utf8` as luajit-utf8-ish helpers)
    loaded.set("re", re_module(lua)?)?;
    let preload: Table = package.get("preload")?;
    preload.set("util", lua.load(UTIL).set_name("=util").into_function()?)?;
    preload.set("karaskel", lua.load(KARASKEL).set_name("=karaskel").into_function()?)?;
    preload.set("aegisub.util", lua.load(UTIL).set_name("=util").into_function()?)?;
    let _ = script;
    Ok(())
}

fn conv_pattern(p: &str, flags: i64) -> String {
    // Aegisub flags: ICASE=1, NOT_BOL... keep simple: re.ICASE = 1
    if flags & 1 != 0 { format!("(?i){}", p) } else { p.to_string() }
}

fn re_module(lua: &Lua) -> mlua::Result<Table> {
    let re = lua.create_table()?;
    re.set("ICASE", 1)?; re.set("NOSUB", 2)?; re.set("NO_MOD_M", 4)?; re.set("MOD_S", 8)?; re.set("MOD_M", 16)?; re.set("NOT_BOL", 32)?; re.set("NOT_EOL", 64)?;
    let make = |lua: &Lua, pat: String, flags: i64| -> mlua::Result<regex::Regex> {
        regex::Regex::new(&conv_pattern(&pat, flags)).map_err(|e| { let _ = lua; mlua::Error::runtime(format!("regex error: {}", e)) })
    };
    // match(s, pat, flags) -> table of {str, first, last} for whole match and groups, or nil
    re.set("match", lua.create_function(move |lua, (s, pat, flags): (String, String, Option<i64>)| {
        let r = make(lua, pat, flags.unwrap_or(0))?;
        match r.captures(&s) {
            None => Ok(Value::Nil),
            Some(c) => { let t = lua.create_table()?; for m in c.iter() { let e = lua.create_table()?; if let Some(m) = m { e.set("str", m.as_str())?; e.set("first", m.start() + 1)?; e.set("last", m.end())?; } t.push(e)?; } Ok(Value::Table(t)) }
        }
    })?)?;
    re.set("find", lua.create_function(move |lua, (s, pat, flags): (String, String, Option<i64>)| {
        let r = make(lua, pat, flags.unwrap_or(0))?;
        let res = lua.create_table()?;
        let mut any = false;
        for m in r.find_iter(&s) { any = true; let e = lua.create_table()?; e.set("str", m.as_str())?; e.set("first", m.start() + 1)?; e.set("last", m.end())?; res.push(e)?; }
        Ok(if any { Value::Table(res) } else { Value::Nil })
    })?)?;
    re.set("sub", lua.create_function(move |lua, (s, pat, rep, count, flags): (String, String, String, Option<usize>, Option<i64>)| {
        let r = make(lua, pat, flags.unwrap_or(0))?;
        let rep = rep.replace("\\", "$"); // \1 -> $1
        let n = count.filter(|&c| c > 0).unwrap_or(0);
        Ok(r.replacen(&s, n, rep.as_str()).into_owned())
    })?)?;
    re.set("gsub", re.get::<Function>("sub")?)?;
    Ok(re)
}
