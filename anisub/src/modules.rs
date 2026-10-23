//! Native replacements for the extra Lua modules Aegisub ships: `unicode` and `re`.

use mlua::{Lua, Table, Value};
use regex::{Regex, RegexBuilder};

const ICASE: i64 = 1;

fn build(pat: &str, flags: i64) -> mlua::Result<Regex> {
    RegexBuilder::new(pat).case_insensitive(flags & ICASE != 0).build().map_err(mlua::Error::external)
}

/// Boost-style replacement (`\1`, `$1`) to the regex crate's `${1}`.
fn convert_repl(r: &str) -> String {
    let mut out = String::new();
    let mut it = r.chars().peekable();
    while let Some(c) = it.next() {
        match c {
            '\\' if it.peek().is_some_and(|d| d.is_ascii_digit()) => {
                out.push_str("${");
                out.push(it.next().unwrap());
                out.push('}');
            }
            '$' if it.peek().is_some_and(|d| d.is_ascii_digit()) => {
                out.push_str("${");
                out.push(it.next().unwrap());
                out.push('}');
            }
            '$' => out.push_str("$$"),
            _ => out.push(c),
        }
    }
    out
}

fn char_index(s: &str, byte: usize) -> usize {
    s[..byte].chars().count()
}

fn match_table(lua: &Lua, s: &str, caps: &regex::Captures, whole_only: bool) -> mlua::Result<Table> {
    let res = lua.create_table()?;
    let n = if whole_only { 1 } else { caps.len() };
    for i in 0..n {
        if let Some(m) = caps.get(i) {
            let t = lua.create_table()?;
            t.set("str", m.as_str())?;
            // Aegisub reports byte offsets (1-based, inclusive).
            t.set("first", m.start() + 1)?;
            t.set("last", m.end())?;
            res.push(t)?;
        } else {
            let t = lua.create_table()?;
            t.set("str", "")?;
            t.set("first", 0)?;
            t.set("last", 0)?;
            res.push(t)?;
        }
    }
    let _ = s;
    Ok(res)
}

fn regex_methods(lua: &Lua, re: Regex, flags: i64) -> mlua::Result<Table> {
    let t = lua.create_table()?;
    let r = re.clone();
    t.set("find", lua.create_function(move |lua, args: mlua::MultiValue| {
        let s = args.iter().rev().find_map(|v| if let Value::String(s) = v { s.to_str().ok().map(|x| x.to_string()) } else { None }).unwrap_or_default();
        let mut out = Vec::new();
        for caps in r.captures_iter(&s) {
            out.push(match_table(lua, &s, &caps, false)?);
        }
        if out.is_empty() { return Ok(Value::Nil); }
        // Aegisub's find returns a flat list of whole-match tables
        let res = lua.create_table()?;
        for caps in r.captures_iter(&s) {
            let m = caps.get(0).unwrap();
            let t = lua.create_table()?;
            t.set("str", m.as_str())?;
            t.set("first", m.start() + 1)?;
            t.set("last", m.end())?;
            res.push(t)?;
        }
        Ok(Value::Table(res))
    })?)?;
    let r = re.clone();
    t.set("match", lua.create_function(move |lua, args: mlua::MultiValue| {
        let s = args.iter().rev().find_map(|v| if let Value::String(s) = v { s.to_str().ok().map(|x| x.to_string()) } else { None }).unwrap_or_default();
        match r.captures(&s) {
            Some(c) => Ok(Value::Table(match_table(lua, &s, &c, false)?)),
            None => Ok(Value::Nil),
        }
    })?)?;
    let r = re.clone();
    t.set("sub", lua.create_function(move |_, args: mlua::MultiValue| {
        let strs: Vec<String> = args.iter().filter_map(|v| if let Value::String(s) = v { s.to_str().ok().map(|x| x.to_string()) } else { None }).collect();
        let count = args.iter().find_map(|v| if let Value::Integer(i) = v { Some(*i as usize) } else { None }).unwrap_or(0);
        let (s, repl) = (strs.first().cloned().unwrap_or_default(), strs.get(1).cloned().unwrap_or_default());
        Ok(r.replacen(&s, count, convert_repl(&repl).as_str()).into_owned())
    })?)?;
    let r = re;
    t.set("split", lua.create_function(move |lua, args: mlua::MultiValue| {
        let s = args.iter().rev().find_map(|v| if let Value::String(s) = v { s.to_str().ok().map(|x| x.to_string()) } else { None }).unwrap_or_default();
        lua.create_sequence_from(r.split(&s).map(|x| x.to_string()))
    })?)?;
    t.set("flags", flags)?;
    Ok(t)
}

pub fn install(lua: &Lua) -> mlua::Result<()> {
    let re = lua.create_table()?;
    re.set("ICASE", ICASE)?;
    re.set("NOSUB", 2)?;
    re.set("NEWLINE", 4)?;
    re.set("compile", lua.create_function(|lua, (p, f): (String, Option<i64>)| {
        let f = f.unwrap_or(0);
        regex_methods(lua, build(&p, f)?, f)
    })?)?;
    // Module-level helpers take the pattern in different positions: find(str, pat), match(pat, str), sub(str, pat, repl)
    re.set("find", lua.create_function(|lua, (s, p, f): (String, String, Option<i64>)| {
        let rx = build(&p, f.unwrap_or(0))?;
        let m = regex_methods(lua, rx, 0)?;
        m.get::<mlua::Function>("find")?.call::<Value>(s)
    })?)?;
    re.set("match", lua.create_function(|lua, (p, s, f): (String, String, Option<i64>)| {
        let rx = build(&p, f.unwrap_or(0))?;
        let m = regex_methods(lua, rx, 0)?;
        m.get::<mlua::Function>("match")?.call::<Value>(s)
    })?)?;
    let sub = lua.create_function(|lua, (s, p, r, c, f): (String, String, String, Option<usize>, Option<i64>)| {
        let rx = build(&p, f.unwrap_or(0))?;
        let m = regex_methods(lua, rx, 0)?;
        m.get::<mlua::Function>("sub")?.call::<String>((s, r, c.unwrap_or(0) as i64))
    })?;
    re.set("sub", sub.clone())?;
    re.set("gsub", sub)?;
    lua.globals().set("re", re)?;

    let u = lua.create_table()?;
    u.set("len", lua.create_function(|_, s: String| Ok(s.chars().count()))?)?;
    u.set("codepoint", lua.create_function(|_, s: String| Ok(s.chars().next().map(|c| c as u32).unwrap_or(0)))?)?;
    u.set("char", lua.create_function(|_, cp: u32| Ok(char::from_u32(cp).map(|c| c.to_string()).unwrap_or_default()))?)?;
    u.set("to_upper_case", lua.create_function(|_, s: String| Ok(s.to_uppercase()))?)?;
    u.set("to_lower_case", lua.create_function(|_, s: String| Ok(s.to_lowercase()))?)?;
    u.set("to_fold_case", lua.create_function(|_, s: String| Ok(s.to_lowercase()))?)?;
    u.set("chars", lua.create_function(|lua, s: String| {
        let chars: Vec<String> = s.chars().map(|c| c.to_string()).collect();
        let i = std::cell::Cell::new(0usize);
        lua.create_function(move |_, ()| {
            let k = i.get();
            i.set(k + 1);
            Ok(chars.get(k).cloned())
        })
    })?)?;
    u.set("sub", lua.create_function(|_, (s, a, b): (String, i64, Option<i64>)| {
        let len = s.chars().count() as i64;
        let norm = |x: i64| if x < 0 { (len + x + 1).max(0) } else { x };
        let (a, b) = (norm(a).max(1), norm(b.unwrap_or(-1)).min(len));
        Ok(if a > b { String::new() } else { s.chars().skip(a as usize - 1).take((b - a + 1) as usize).collect() })
    })?)?;
    lua.globals().set("unicode", u)?;
    let _ = char_index;
    Ok(())
}
