//! `re` module (Boost-regex flavored API on top of fancy-regex) for Automation scripts.

use fancy_regex::{Regex, RegexBuilder};
use mlua::{Lua, Table, Value};

const ICASE: i64 = 1;
const MOD_X: i64 = 16;
const MOD_S: i64 = 8;

fn build(pat: &str, flags: i64) -> mlua::Result<Regex> {
    let mut b = RegexBuilder::new(pat);
    b.case_insensitive(flags & ICASE != 0);
    b.verbose_mode(flags & MOD_X != 0);
    let _ = MOD_S;
    b.build().map_err(|e| mlua::Error::runtime(format!("regex error: {}", e)))
}

fn m_table(lua: &Lua, s: &str, start: usize, end: usize) -> mlua::Result<Table> {
    let t = lua.create_table()?;
    t.set("str", &s[start..end])?;
    t.set("first", start + 1)?;
    t.set("last", end)?;
    Ok(t)
}

fn expand(repl: &str, caps: &fancy_regex::Captures<str>) -> String {
    let mut out = String::new();
    let b: Vec<char> = repl.chars().collect();
    let mut i = 0;
    while i < b.len() {
        let c = b[i];
        if (c == '\\' || c == '$') && i + 1 < b.len() && b[i + 1].is_ascii_digit() {
            let n = b[i + 1].to_digit(10).unwrap() as usize;
            if let Some(m) = caps.get(n) {
                out.push_str(m.as_str());
            }
            i += 2;
        } else if c == '\\' && i + 1 < b.len() && (b[i + 1] == '\\' || b[i + 1] == '$') {
            out.push(b[i + 1]);
            i += 2;
        } else {
            out.push(c);
            i += 1;
        }
    }
    out
}

pub fn install(lua: &Lua, sf: &Table) -> mlua::Result<()> {
    sf.set(
        "re_find",
        lua.create_function(|lua, (s, p, f): (String, String, i64)| {
            let re = build(&p, f)?;
            let out = lua.create_table()?;
            let mut n = 0;
            for m in re.find_iter(&s) {
                let m = m.map_err(|e| mlua::Error::runtime(e.to_string()))?;
                n += 1;
                out.set(n, m_table(lua, &s, m.start(), m.end())?)?;
            }
            Ok(if n == 0 { Value::Nil } else { Value::Table(out) })
        })?,
    )?;
    sf.set(
        "re_match",
        lua.create_function(|lua, (s, p, f): (String, String, i64)| {
            let re = build(&p, f)?;
            let caps = re.captures(&s).map_err(|e| mlua::Error::runtime(e.to_string()))?;
            let Some(caps) = caps else { return Ok(Value::Nil) };
            let out = lua.create_table()?;
            for i in 0..caps.len() {
                match caps.get(i) {
                    Some(m) => out.set(i + 1, m_table(lua, &s, m.start(), m.end())?)?,
                    None => {
                        let t = lua.create_table()?;
                        t.set("str", "")?;
                        t.set("first", 0)?;
                        t.set("last", -1)?;
                        out.set(i + 1, t)?
                    }
                }
            }
            Ok(Value::Table(out))
        })?,
    )?;
    sf.set(
        "re_sub",
        lua.create_function(|_, (s, p, r, count, f): (String, String, String, i64, i64)| {
            let re = build(&p, f)?;
            let mut out = String::new();
            let mut last = 0;
            let mut done = 0;
            for caps in re.captures_iter(s.as_str()) {
                let caps = caps.map_err(|e| mlua::Error::runtime(e.to_string()))?;
                let m = caps.get(0).unwrap();
                out.push_str(&s[last..m.start()]);
                out.push_str(&expand(&r, &caps));
                last = m.end();
                done += 1;
                if count > 0 && done >= count {
                    break;
                }
            }
            out.push_str(&s[last..]);
            Ok((out, done))
        })?,
    )?;
    sf.set(
        "re_split",
        lua.create_function(|lua, (s, p, f): (String, String, i64)| {
            let re = build(&p, f)?;
            let out = lua.create_table()?;
            let mut last = 0;
            let mut n = 0;
            for m in re.find_iter(&s) {
                let m = m.map_err(|e| mlua::Error::runtime(e.to_string()))?;
                n += 1;
                out.set(n, &s[last..m.start()])?;
                last = m.end();
            }
            out.set(n + 1, &s[last..])?;
            Ok(out)
        })?,
    )?;
    Ok(())
}
