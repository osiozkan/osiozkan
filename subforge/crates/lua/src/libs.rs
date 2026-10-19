//! Native modules exposed to scripts: `unicode` and `re`.
use fancy_regex::Regex;
use mlua::{Function, Lua, Result, Table, UserData, UserDataMethods, Value, Variadic};
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use subforge_core::turkish;

pub fn install_unicode(lua: &Lua) -> Result<Table> {
    let t = lua.create_table()?;
    t.set(
        "codepoint",
        lua.create_function(|_, s: String| Ok(s.chars().next().map(|c| c as u32).unwrap_or(0)))?,
    )?;
    t.set("len", lua.create_function(|_, s: mlua::LuaString| Ok(String::from_utf8_lossy(&s.as_bytes()).chars().count()))?)?;
    t.set(
        "chars",
        lua.create_function(|lua, s: mlua::LuaString| {
            let st = String::from_utf8_lossy(&s.as_bytes()).into_owned();
            let chars: Vec<String> = st.chars().map(|c| c.to_string()).collect();
            let idx = Rc::new(RefCell::new(0usize));
            lua.create_function(move |_, ()| {
                let mut i = idx.borrow_mut();
                let r = chars.get(*i).cloned();
                *i += 1;
                Ok(r)
            })
        })?,
    )?;
    t.set("to_upper", lua.create_function(|_, s: String| Ok(s.to_uppercase()))?)?;
    t.set("to_lower", lua.create_function(|_, s: String| Ok(s.to_lowercase()))?)?;
    t.set("to_fold", lua.create_function(|_, s: String| Ok(s.to_lowercase()))?)?;
    // Turkish-aware variants (SubForge extension).
    t.set("to_upper_tr", lua.create_function(|_, s: String| Ok(turkish::upper(&s)))?)?;
    t.set("to_lower_tr", lua.create_function(|_, s: String| Ok(turkish::lower(&s)))?)?;
    t.set("title_tr", lua.create_function(|_, s: String| Ok(turkish::title(&s)))?)?;
    t.set(
        "charwidth",
        lua.create_function(|_, s: String| Ok(s.chars().next().map(|c| if (c as u32) >= 0x2E80 { 2 } else { 1 }).unwrap_or(0)))?,
    )?;
    Ok(t)
}

// ------------------------------------------------------------------ re

#[derive(Clone)]
struct ReObj(Rc<Regex>);

fn build(pattern: &str, flags: &[String]) -> Result<Regex> {
    let mut prefix = String::new();
    for f in flags {
        match f.to_ascii_uppercase().as_str() {
            "ICASE" | "I" => prefix.push_str("(?i)"),
            "MULTILINE" | "M" => prefix.push_str("(?m)"),
            "DOT_MATCH_ALL" | "S" => prefix.push_str("(?s)"),
            _ => {}
        }
    }
    let full = format!("{prefix}{pattern}");
    Regex::new(&full).map_err(|e| mlua::Error::runtime(format!("re: invalid pattern '{pattern}': {e}")))
}

fn caps_to_matches(lua: &Lua, re: &Regex, subject: &str, first_only: bool) -> Result<Value> {
    let out = lua.create_table()?;
    let mut n = 0;
    for caps in re.captures_iter(subject) {
        let caps = caps.map_err(|e| mlua::Error::runtime(e.to_string()))?;
        let m = lua.create_table()?;
        for (i, c) in caps.iter().enumerate() {
            let e = lua.create_table()?;
            match c {
                Some(c) => {
                    e.set("str", c.as_str())?;
                    // 1-based inclusive byte offsets, as Aegisub reports them
                    e.set("first", c.start() + 1)?;
                    e.set("last", c.end())?;
                }
                None => {
                    e.set("str", "")?;
                    e.set("first", 0)?;
                    e.set("last", 0)?;
                }
            }
            m.set(i + 1, e)?;
        }
        n += 1;
        out.set(n, m)?;
        if first_only {
            break;
        }
    }
    if n == 0 {
        Ok(Value::Nil)
    } else {
        Ok(Value::Table(out))
    }
}

fn convert_repl(r: &str) -> String {
    // Aegisub/Boost: \1 or $1 → backreference. fancy_regex wants ${1}.
    let mut out = String::new();
    let cs: Vec<char> = r.chars().collect();
    let mut i = 0;
    while i < cs.len() {
        let c = cs[i];
        if (c == '\\' || c == '$') && i + 1 < cs.len() && cs[i + 1].is_ascii_digit() {
            out.push_str("${");
            out.push(cs[i + 1]);
            out.push('}');
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

fn sub(re: &Regex, subject: &str, repl: &str, count: Option<usize>) -> Result<String> {
    let r = convert_repl(repl);
    let res = match count {
        Some(n) if n > 0 => re.replacen(subject, n, r.as_str()),
        _ => re.replace_all(subject, r.as_str()),
    };
    Ok(res.into_owned())
}

impl UserData for ReObj {
    fn add_methods<M: UserDataMethods<Self>>(m: &mut M) {
        m.add_method("find", |lua, this, s: String| caps_to_matches(lua, &this.0, &s, false));
        m.add_method("match", |lua, this, s: String| caps_to_matches(lua, &this.0, &s, true));
        m.add_method("sub", |_, this, (s, repl, count): (String, String, Option<usize>)| sub(&this.0, &s, &repl, count));
        m.add_method("gsub", |_, this, (s, repl, count): (String, String, Option<usize>)| sub(&this.0, &s, &repl, count));
        m.add_method("split", |lua, this, s: String| {
            let t = lua.create_table()?;
            let mut last = 0;
            let mut n = 0;
            for m in this.0.find_iter(&s).flatten() {
                n += 1;
                t.set(n, &s[last..m.start()])?;
                last = m.end();
            }
            t.set(n + 1, &s[last..])?;
            Ok(t)
        });
    }
}

pub fn install_re(lua: &Lua) -> Result<Table> {
    let t = lua.create_table()?;
    let cache: Rc<RefCell<HashMap<String, Rc<Regex>>>> = Rc::new(RefCell::new(HashMap::new()));
    let get = {
        let cache = cache.clone();
        move |p: &str, flags: &[String]| -> Result<Rc<Regex>> {
            let key = format!("{}\u{0}{}", flags.join(","), p);
            if let Some(r) = cache.borrow().get(&key) {
                return Ok(r.clone());
            }
            let r = Rc::new(build(p, flags)?);
            cache.borrow_mut().insert(key, r.clone());
            Ok(r)
        }
    };
    let flags_of = |v: Variadic<String>| -> Vec<String> { v.into_iter().collect() };

    {
        let get = get.clone();
        t.set(
            "compile",
            lua.create_function(move |_, (p, flags): (String, Variadic<String>)| Ok(ReObj(get(&p, &flags_of(flags))?)))?,
        )?;
    }
    for name in ["find", "match"] {
        let get = get.clone();
        let first = name == "match";
        t.set(
            name,
            lua.create_function(move |lua, (s, p, flags): (String, String, Variadic<String>)| {
                let re = get(&p, &flags_of(flags))?;
                caps_to_matches(lua, &re, &s, first)
            })?,
        )?;
    }
    for name in ["sub", "gsub"] {
        let get = get.clone();
        t.set(
            name,
            lua.create_function(move |_, (s, p, r, count): (String, String, String, Option<usize>)| {
                let re = get(&p, &[])?;
                sub(&re, &s, &r, count)
            })?,
        )?;
    }
    Ok(t)
}

pub fn preload(lua: &Lua, name: &str, value: Table) -> Result<()> {
    let package: Table = lua.globals().get("package")?;
    let loaded: Table = package.get("loaded")?;
    loaded.set(name, value)?;
    Ok(())
}

#[allow(dead_code)]
fn _assert(_: Function) {}
