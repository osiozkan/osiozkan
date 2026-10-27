//! Native helper modules exposed to Lua: `unicode`, `re`, and text measuring.

use fancy_regex::{Regex, RegexBuilder};
use mlua::{Lua, MultiValue, Result, Table, Value};
use std::cell::RefCell;
use std::collections::HashMap;

thread_local! {
    static CACHE: RefCell<HashMap<(String, u32), Regex>> = RefCell::new(HashMap::new());
}

const ICASE: u32 = 1;
const NOSUB: u32 = 2;

fn flags_of(v: &Option<Value>) -> u32 {
    // Aegisub: re.ICASE, re.NOSUB, re.NEWLINE… we implement ICASE and ignore the rest.
    match v {
        Some(Value::Integer(i)) => *i as u32,
        Some(Value::Number(f)) => *f as u32,
        _ => 0,
    }
}

fn compile(pat: &str, flags: u32) -> Result<Regex> {
    if let Some(r) = CACHE.with(|c| c.borrow().get(&(pat.to_string(), flags)).cloned()) {
        return Ok(r);
    }
    let r = RegexBuilder::new(pat).case_insensitive(flags & ICASE != 0).build().map_err(|e| mlua::Error::runtime(format!("regex error: {e}")))?;
    CACHE.with(|c| c.borrow_mut().insert((pat.to_string(), flags), r.clone()));
    Ok(r)
}

fn mk_match(lua: &Lua, s: &str, start: usize, end: usize) -> Result<Table> {
    let t = lua.create_table()?;
    t.set("str", &s[start..end])?;
    t.set("first", start + 1)?;
    t.set("last", end)?;
    Ok(t)
}

fn captures_table(lua: &Lua, s: &str, caps: &fancy_regex::Captures) -> Result<Table> {
    let out = lua.create_table()?;
    for i in 0..caps.len() {
        if let Some(m) = caps.get(i) {
            out.push(mk_match(lua, s, m.start(), m.end())?)?;
        } else {
            let t = lua.create_table()?;
            t.set("str", "")?;
            t.set("first", 0)?;
            t.set("last", 0)?;
            out.push(t)?;
        }
    }
    Ok(out)
}

fn convert_repl(r: &str) -> String {
    // Aegisub uses Boost format: $1, \1, $& … Convert to fancy-regex: ${1}
    let mut out = String::new();
    let cs: Vec<char> = r.chars().collect();
    let mut i = 0;
    while i < cs.len() {
        let c = cs[i];
        if (c == '\\' || c == '$') && i + 1 < cs.len() && cs[i + 1].is_ascii_digit() {
            out.push_str(&format!("${{{}}}", cs[i + 1]));
            i += 2;
        } else if c == '$' && i + 1 < cs.len() && cs[i + 1] == '&' {
            out.push_str("${0}");
            i += 2;
        } else if c == '$' {
            out.push_str("$$");
            i += 1;
        } else {
            out.push(c);
            i += 1;
        }
    }
    out
}

pub fn install_re(lua: &Lua) -> Result<Table> {
    let re = lua.create_table()?;
    re.set("ICASE", ICASE)?;
    re.set("NOSUB", NOSUB)?;
    re.set("BASIC", 0)?;
    re.set("PERL", 0)?;
    re.set("EXTENDED", 0)?;
    re.set("NO_MOD_M", 0)?;
    re.set("MOD_S", 0)?;
    re.set("MOD_X", 0)?;
    re.set("NEWLINE", 0)?;

    // Core functions shared between module-level and compiled-object calls.
    let find = lua.create_function(|lua, (s, pat, fl): (String, String, Option<Value>)| {
        let r = compile(&pat, flags_of(&fl))?;
        let out = lua.create_table()?;
        let mut n = 0;
        for m in r.find_iter(&s) {
            let m = m.map_err(|e| mlua::Error::runtime(e.to_string()))?;
            out.push(mk_match(lua, &s, m.start(), m.end())?)?;
            n += 1;
        }
        if n == 0 { Ok(Value::Nil) } else { Ok(Value::Table(out)) }
    })?;
    let matchf = lua.create_function(|lua, (s, pat, fl): (String, String, Option<Value>)| {
        let r = compile(&pat, flags_of(&fl))?;
        match r.captures(&s).map_err(|e| mlua::Error::runtime(e.to_string()))? {
            Some(c) => Ok(Value::Table(captures_table(lua, &s, &c)?)),
            None => Ok(Value::Nil),
        }
    })?;
    let sub = lua.create_function(|_, (s, pat, rep, count, fl): (String, String, Value, Option<i64>, Option<Value>)| {
        let r = compile(&pat, flags_of(&fl))?;
        let limit = count.filter(|&c| c > 0).unwrap_or(0) as usize;
        match rep {
            Value::String(rp) => {
                let rp = convert_repl(&rp.to_str()?);
                Ok(r.replacen(&s, limit, rp.as_str()).into_owned())
            }
            _ => Err(mlua::Error::runtime("re.sub: replacement must be a string")),
        }
    })?;
    let split = lua.create_function(|lua, (s, pat, fl): (String, String, Option<Value>)| {
        let r = compile(&pat, flags_of(&fl))?;
        let out = lua.create_table()?;
        let mut last = 0;
        for m in r.find_iter(&s) {
            let m = m.map_err(|e| mlua::Error::runtime(e.to_string()))?;
            out.push(&s[last..m.start()])?;
            last = m.end();
        }
        out.push(&s[last..])?;
        Ok(out)
    })?;
    re.set("find", find.clone())?;
    re.set("match", matchf.clone())?;
    re.set("sub", sub.clone())?;
    re.set("gsub", sub.clone())?;
    re.set("split", split.clone())?;
    let compile_fn = lua.create_function(move |lua, (pat, fl): (String, Option<Value>)| {
        compile(&pat, flags_of(&fl))?; // validate
        let obj = lua.create_table()?;
        let (find, matchf, sub, split) = (find.clone(), matchf.clone(), sub.clone(), split.clone());
        let p = pat.clone();
        let mk = |f: mlua::Function| -> Result<mlua::Function> {
            let p = p.clone();
            lua.create_function(move |lua, args: MultiValue| {
                // obj:method(str, ...) → f(str, pat, ...flags)
                let mut it = args.into_iter();
                it.next();
                let s = it.next().unwrap_or(Value::Nil);
                let rest: Vec<Value> = it.collect();
                let mut v = vec![s, Value::String(lua.create_string(&p)?)];
                v.extend(rest);
                f.call::<Value>(MultiValue::from_vec(v))
            })
        };
        obj.set("find", mk(find)?)?;
        obj.set("match", mk(matchf)?)?;
        obj.set("sub", mk(sub.clone())?)?;
        obj.set("gsub", mk(sub)?)?;
        obj.set("split", mk(split)?)?;
        Ok(obj)
    })?;
    re.set("compile", compile_fn)?;
    Ok(re)
}

pub fn install_unicode(lua: &Lua) -> Result<Table> {
    let u = lua.create_table()?;
    u.set("codepoint", lua.create_function(|_, s: String| Ok(s.chars().next().map(|c| c as u32).unwrap_or(0)))?)?;
    u.set("len", lua.create_function(|_, s: String| Ok(s.chars().count()))?)?;
    u.set("to_upper", lua.create_function(|_, s: String| Ok(s.to_uppercase()))?)?;
    u.set("to_lower", lua.create_function(|_, s: String| Ok(s.to_lowercase()))?)?;
    u.set("to_fold", lua.create_function(|_, s: String| Ok(s.to_lowercase()))?)?;
    u.set(
        "chars",
        lua.create_function(|lua, s: String| {
            let chars: Vec<String> = s.chars().map(|c| c.to_string()).collect();
            let mut i = 0usize;
            lua.create_function_mut(move |_, ()| {
                if i < chars.len() {
                    i += 1;
                    Ok(Some(chars[i - 1].clone()))
                } else {
                    Ok(None)
                }
            })
        })?,
    )?;
    Ok(u)
}

/// Approximate text extents without a font backend: width/height/descent/extlead.
/// Heuristic: average glyph advance ≈ 0.52·size (0.6 for monospace-looking names, 0.35 for narrow punctuation/spaces).
pub fn approx_extents(fontsize: f64, scale_x: f64, scale_y: f64, spacing: f64, bold: bool, text: &str) -> (f64, f64, f64, f64) {
    let mut w = 0.0;
    for c in text.chars() {
        let adv = match c {
            ' ' | 'i' | 'l' | 'j' | '.' | ',' | ':' | ';' | '\'' | '!' | '|' | 'I' | 't' | 'f' | 'r' => 0.30,
            'm' | 'w' | 'M' | 'W' | '@' => 0.80,
            c if (c as u32) >= 0x2E80 => 1.0, // CJK full width
            c if c.is_uppercase() => 0.66,
            _ => 0.52,
        };
        w += adv * fontsize * if bold { 1.05 } else { 1.0 } + spacing;
    }
    let w = w * scale_x / 100.0;
    let h = fontsize * 1.15 * scale_y / 100.0;
    let d = fontsize * 0.22 * scale_y / 100.0;
    (w, h, d, 0.0)
}

/// Minimal `utf8` library (lua-utf8 flavoured): char, len, codepoint, sub, reverse, upper, lower.
pub fn install_utf8(lua: &Lua) -> Result<Table> {
    let u = lua.create_table()?;
    u.set("char", lua.create_function(|_, args: MultiValue| {
        let mut s = String::new();
        for a in args {
            let n = match a { Value::Integer(i) => i as u32, Value::Number(f) => f as u32, _ => 0 };
            s.push(char::from_u32(n).unwrap_or('\u{FFFD}'));
        }
        Ok(s)
    })?)?;
    u.set("len", lua.create_function(|_, s: String| Ok(s.chars().count()))?)?;
    u.set("upper", lua.create_function(|_, s: String| Ok(s.to_uppercase()))?)?;
    u.set("lower", lua.create_function(|_, s: String| Ok(s.to_lowercase()))?)?;
    u.set("reverse", lua.create_function(|_, s: String| Ok(s.chars().rev().collect::<String>()))?)?;
    u.set("codepoint", lua.create_function(|_, (s, i, j): (String, Option<i64>, Option<i64>)| {
        let cs: Vec<char> = s.chars().collect();
        let n = cs.len() as i64;
        let i = i.unwrap_or(1);
        let i = if i < 0 { n + i + 1 } else { i }.max(1);
        let j = j.unwrap_or(i);
        let j = if j < 0 { n + j + 1 } else { j }.min(n);
        let mut out = Vec::new();
        let mut k = i;
        while k <= j {
            out.push(Value::Integer(cs[(k - 1) as usize] as i64));
            k += 1;
        }
        Ok(MultiValue::from_vec(out))
    })?)?;
    u.set("sub", lua.create_function(|_, (s, i, j): (String, i64, Option<i64>)| {
        let cs: Vec<char> = s.chars().collect();
        let n = cs.len() as i64;
        let i = if i < 0 { n + i + 1 } else { i }.max(1);
        let j = j.unwrap_or(-1);
        let j = if j < 0 { n + j + 1 } else { j }.min(n);
        Ok(if i > j { String::new() } else { cs[(i - 1) as usize..j as usize].iter().collect() })
    })?)?;
    Ok(u)
}

/// Minimal LuaFileSystem: attributes, dir, mkdir, rmdir, currentdir, chdir.
pub fn install_lfs(lua: &Lua) -> Result<Table> {
    let t = lua.create_table()?;
    t.set("currentdir", lua.create_function(|_, ()| Ok(std::env::current_dir().map(|p| p.to_string_lossy().to_string()).ok()))?)?;
    t.set("chdir", lua.create_function(|_, p: String| Ok(std::env::set_current_dir(p).is_ok()))?)?;
    t.set("mkdir", lua.create_function(|_, p: String| Ok(std::fs::create_dir(p).is_ok()))?)?;
    t.set("rmdir", lua.create_function(|_, p: String| Ok(std::fs::remove_dir(p).is_ok()))?)?;
    t.set("attributes", lua.create_function(|lua, (p, attr): (String, Option<String>)| {
        let Ok(m) = std::fs::metadata(&p) else { return Ok(Value::Nil) };
        let out = lua.create_table()?;
        out.set("mode", if m.is_dir() { "directory" } else { "file" })?;
        out.set("size", m.len())?;
        let secs = |t: std::io::Result<std::time::SystemTime>| t.ok().and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map(|d| d.as_secs()).unwrap_or(0);
        out.set("modification", secs(m.modified()))?;
        out.set("access", secs(m.accessed()))?;
        out.set("change", secs(m.modified()))?;
        match attr {
            Some(a) => out.get::<Value>(a),
            None => Ok(Value::Table(out)),
        }
    })?)?;
    t.set("dir", lua.create_function(|lua, p: String| {
        let mut names: Vec<String> = vec![".".into(), "..".into()];
        if let Ok(rd) = std::fs::read_dir(&p) {
            names.extend(rd.flatten().map(|e| e.file_name().to_string_lossy().to_string()));
        }
        let mut i = 0usize;
        lua.create_function_mut(move |_, ()| {
            i += 1;
            Ok(names.get(i - 1).cloned())
        })
    })?)?;
    Ok(t)
}
