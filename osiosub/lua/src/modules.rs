//! Bundled Lua modules: util, karaskel, utf8 (Lua), unicode and re (Rust).
use mlua::{Function, Lua, Result, Table, Value};
use regex::RegexBuilder;

const UTIL: &str = include_str!("../lib/util.lua");
const KARASKEL: &str = include_str!("../lib/karaskel.lua");
const UTF8: &str = include_str!("../lib/utf8.lua");

fn preload(lua: &Lua, name: &str, src: &'static str) -> Result<()> {
    let pkg: Table = lua.globals().get("package")?;
    let pre: Table = pkg.get("preload")?;
    let nm = name.to_string();
    pre.set(name, lua.create_function(move |lua, _: mlua::MultiValue| -> Result<Value> {
        lua.load(src).set_name(format!("={}", nm)).eval::<Value>()
    })?)?;
    Ok(())
}

pub fn install(lua: &Lua) -> Result<()> {
    preload(lua, "util", UTIL)?;
    preload(lua, "karaskel", KARASKEL)?;
    preload(lua, "utf8", UTF8)?;
    // utils-auto4 alias to util for scripts that include it
    preload(lua, "utils-auto4", "return require('util')")?;
    preload(lua, "utils", "return require('util')")?;

    // ---- unicode ----
    let u = lua.create_table()?;
    u.set("len", lua.create_function(|_, s: mlua::String| Ok(String::from_utf8_lossy(&s.as_bytes()).chars().count()))?)?;
    u.set("to_upper", lua.create_function(|_, s: String| Ok(s.to_uppercase()))?)?;
    u.set("to_lower", lua.create_function(|_, s: String| Ok(s.to_lowercase()))?)?;
    u.set("to_fold_case", lua.create_function(|_, s: String| Ok(s.to_lowercase()))?)?;
    u.set("codepoint", lua.create_function(|_, s: String| Ok(s.chars().next().map(|c| c as u32).unwrap_or(0)))?)?;
    u.set("chars", lua.create_function(|lua, s: String| {
        let v: Vec<String> = s.chars().map(|c| c.to_string()).collect();
        let mut i = 0usize;
        lua.create_function_mut(move |_, ()| { i += 1; Ok(v.get(i - 1).cloned()) })
    })?)?;
    lua.globals().set("unicode", u)?;

    // ---- re (Boost.Regex-like subset on top of Rust regex) ----
    let re = lua.create_table()?;
    fn flags(f: Option<Value>) -> (bool, bool) {
        // returns (ignore_case, extended); flags: re.ICASE=1, re.NOSUB, re.EXTENDED...
        match f { Some(Value::Integer(i)) => (i & 1 != 0, i & 2 != 0), Some(Value::Number(n)) => ((n as i64) & 1 != 0, (n as i64) & 2 != 0), _ => (false, false) }
    }
    fn build(pat: &str, f: Option<Value>) -> std::result::Result<regex::Regex, mlua::Error> {
        let (ic, ext) = flags(f);
        RegexBuilder::new(pat).case_insensitive(ic).ignore_whitespace(ext).build().map_err(|e| mlua::Error::runtime(e.to_string()))
    }
    re.set("ICASE", 1)?; re.set("NOSUB", 4)?; re.set("EXTENDED", 2)?; re.set("NO_MOD_M", 0)?;
    re.set("find", lua.create_function(|lua, (s, pat, f): (String, String, Option<Value>)| {
        let r = build(&pat, f)?;
        let out = lua.create_table()?;
        let mut n = 0;
        for c in r.captures_iter(&s) {
            let m = c.get(0).unwrap();
            let t = lua.create_table()?;
            t.set("str", m.as_str())?; t.set("first", s[..m.start()].chars().count() + 1)?; t.set("last", s[..m.end()].chars().count())?;
            n += 1; out.set(n, t)?;
            for g in 1..c.len() {
                if let Some(g) = c.get(g) {
                    let t = lua.create_table()?;
                    t.set("str", g.as_str())?; t.set("first", s[..g.start()].chars().count() + 1)?; t.set("last", s[..g.end()].chars().count())?;
                    n += 1; out.set(n, t)?;
                }
            }
            // Aegisub's re.find returns only the first match with sub-captures following; stop at first
            break;
        }
        Ok(if n == 0 { Value::Nil } else { Value::Table(out) })
    })?)?;
    re.set("match", lua.create_function(|lua, (s, pat, f): (String, String, Option<Value>)| {
        let r = build(&pat, f)?;
        let out = lua.create_table()?;
        let mut n = 0;
        if let Some(c) = r.captures(&s) {
            for g in 0..c.len() {
                if let Some(g) = c.get(g) {
                    let t = lua.create_table()?;
                    t.set("str", g.as_str())?; t.set("first", s[..g.start()].chars().count() + 1)?; t.set("last", s[..g.end()].chars().count())?;
                    n += 1; out.set(n, t)?;
                }
            }
        }
        Ok(if n == 0 { Value::Nil } else { Value::Table(out) })
    })?)?;
    re.set("sub", lua.create_function(|_, (s, pat, rep, count, f): (String, String, String, Option<i64>, Option<Value>)| {
        let r = build(&pat, f)?;
        // convert Boost/ECMAScript style \1 and $1 -> ${1}
        let conv = regex::Regex::new(r"\\(\d)").unwrap().replace_all(&rep, "$${$1}").into_owned();
        let limit = count.unwrap_or(0).max(0) as usize;
        Ok(r.replacen(&s, limit, conv.as_str()).into_owned())
    })?)?;
    re.set("gsub", lua.create_function(|_, (s, pat, rep, f): (String, String, String, Option<Value>)| {
        let r = build(&pat, f)?;
        let conv = regex::Regex::new(r"\\(\d)").unwrap().replace_all(&rep, "$${$1}").into_owned();
        Ok(r.replace_all(&s, conv.as_str()).into_owned())
    })?)?;
    re.set("split", lua.create_function(|lua, (s, pat): (String, String)| {
        let r = build(&pat, None)?;
        let t = lua.create_table()?;
        for (i, p) in r.split(&s).enumerate() { t.set(i + 1, p)?; }
        Ok(t)
    })?)?;
    lua.globals().set("re", re)?;
    // make `require "re"` / `require "unicode"` work like in Aegisub
    let pkg: Table = lua.globals().get("package")?;
    let loaded: Table = pkg.get("loaded")?;
    loaded.set("re", lua.globals().get::<Value>("re")?)?;
    loaded.set("unicode", lua.globals().get::<Value>("unicode")?)?;
    let _: Option<Function> = None;
    lua.load("utf8 = require('utf8')").exec()?;
    Ok(())
}
