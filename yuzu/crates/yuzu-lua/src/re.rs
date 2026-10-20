//! Aegisub `re` module on top of the Rust regex engine.

use mlua::{Function, Lua, Result as LResult, Table, Value};
use regex::{Regex, RegexBuilder};

const ICASE: i64 = 1;
const NOSUB: i64 = 2;
const EXTENDED: i64 = 4;
const NEWLINE_ALT: i64 = 8;

fn build(pat: &str, flags: i64) -> LResult<Regex> {
    let mut b = RegexBuilder::new(pat);
    b.case_insensitive(flags & ICASE != 0).ignore_whitespace(flags & EXTENDED != 0);
    b.build().map_err(|e| mlua::Error::runtime(format!("invalid regex: {}", e)))
}

/// Convert Perl-ish replacement (`\1`, `$1`, `\0`) to Rust `${n}` syntax.
fn conv_repl(r: &str) -> String {
    let mut o = String::new();
    let cs: Vec<char> = r.chars().collect();
    let mut i = 0;
    while i < cs.len() {
        let c = cs[i];
        if (c == '\\' || c == '$') && i + 1 < cs.len() && cs[i + 1].is_ascii_digit() {
            o.push_str(&format!("${{{}}}", cs[i + 1]));
            i += 2;
        } else if c == '$' {
            o.push_str("$$");
            i += 1;
        } else {
            o.push(c);
            i += 1;
        }
    }
    o
}

fn match_tables(lua: &Lua, re: &Regex, s: &str, all: bool) -> LResult<Value> {
    let out = lua.create_table()?;
    let mut n = 0;
    // Aegisub returns, per match, a table of captures: [1] = whole match, [2..] = groups.
    for caps in re.captures_iter(s) {
        for g in caps.iter() {
            n += 1;
            let t = lua.create_table()?;
            match g {
                Some(m) => {
                    t.set("str", m.as_str())?;
                    t.set("first", s[..m.start()].chars().count() as i64 + 1)?;
                    t.set("last", s[..m.end()].chars().count() as i64)?;
                }
                None => {
                    t.set("str", "")?;
                    t.set("first", 0)?;
                    t.set("last", -1)?;
                }
            }
            out.set(n, t)?;
        }
        if !all {
            break;
        }
    }
    if n == 0 { Ok(Value::Nil) } else { Ok(Value::Table(out)) }
}

pub fn install(lua: &Lua) -> LResult<()> {
    let re = lua.create_table()?;
    re.set("ICASE", ICASE)?;
    re.set("NOSUB", NOSUB)?;
    re.set("EXTENDED", EXTENDED)?;
    re.set("NEWLINE_ALT", NEWLINE_ALT)?;
    re.set("find", lua.create_function(|lua, (s, p, f): (String, String, Option<i64>)| {
        let r = build(&p, f.unwrap_or(0))?;
        // find returns one entry per match: the table {str, first, last} of the whole match
        let out = lua.create_table()?;
        let mut n = 0;
        for m in r.find_iter(&s) {
            n += 1;
            let t = lua.create_table()?;
            t.set("str", m.as_str())?;
            t.set("first", s[..m.start()].chars().count() as i64 + 1)?;
            t.set("last", s[..m.end()].chars().count() as i64)?;
            out.set(n, t)?;
        }
        if n == 0 { Ok(Value::Nil) } else { Ok(Value::Table(out)) }
    })?)?;
    re.set("match", lua.create_function(|lua, (s, p, f): (String, String, Option<i64>)| {
        let r = build(&p, f.unwrap_or(0))?;
        match_tables(lua, &r, &s, false)
    })?)?;
    re.set("sub", lua.create_function(|_, (s, p, rep, count, f): (String, String, Value, Option<i64>, Option<i64>)| {
        let r = build(&p, f.unwrap_or(0))?;
        let limit = count.filter(|c| *c > 0).map(|c| c as usize).unwrap_or(0);
        match rep {
            Value::String(rs) => {
                let rs = conv_repl(&rs.to_str()?);
                Ok(r.replacen(&s, limit, rs.as_str()).into_owned())
            }
            Value::Function(func) => {
                let func: Function = func;
                let mut err = None;
                let out = r.replacen(&s, limit, |c: &regex::Captures| {
                    let m = c.get(0).unwrap();
                    match func.call::<Value>(m.as_str()) {
                        Ok(Value::String(x)) => x.to_str().map(|x| x.to_string()).unwrap_or_default(),
                        Ok(_) => m.as_str().to_string(),
                        Err(e) => { err = Some(e); String::new() }
                    }
                });
                if let Some(e) = err { return Err(e); }
                Ok(out.into_owned())
            }
            _ => Err(mlua::Error::runtime("re.sub: replacement must be a string or function")),
        }
    })?)?;
    re.set("split", lua.create_function(|lua, (s, p, skip_empty, max, f): (String, String, Option<bool>, Option<i64>, Option<i64>)| {
        let r = build(&p, f.unwrap_or(0))?;
        let out = lua.create_table()?;
        let mut i = 0;
        let lim = max.filter(|m| *m > 0).map(|m| m as usize).unwrap_or(usize::MAX);
        let parts: Vec<&str> = if lim == usize::MAX { r.split(&s).collect() } else { r.splitn(&s, lim).collect() };
        for part in parts {
            if skip_empty.unwrap_or(false) && part.is_empty() { continue; }
            i += 1;
            out.set(i, part)?;
        }
        Ok(out)
    })?)?;
    lua.globals().set("__yuzu_re", re.clone())?;
    let pkg: Table = lua.globals().get("package")?;
    let loaded: Table = pkg.get("loaded")?;
    loaded.set("re", re.clone())?;
    loaded.set("aegisub.re", re)?;
    Ok(())
}

