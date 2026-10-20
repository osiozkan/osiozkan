//! Regex helpers backing the Aegisub-style `re` module (see lua/prelude.lua).

use mlua::{Lua, Result, Table};
use regex::RegexBuilder;

fn build(pattern: &str, flags: i64) -> std::result::Result<regex::Regex, mlua::Error> {
    let icase = flags & 1 != 0;
    RegexBuilder::new(pattern)
        .case_insensitive(icase)
        .multi_line(flags & 2 == 0)
        .dot_matches_new_line(flags & 4 != 0)
        .ignore_whitespace(flags & 8 != 0)
        .build()
        .map_err(|e| mlua::Error::runtime(format!("invalid regex '{}': {}", pattern, e)))
}

pub(crate) fn install(lua: &Lua) -> Result<()> {
    let g = lua.globals();

    // __re_matches(pattern, flags, subject, max) -> { {cap1, cap2...}, ... } ; cap = {str=, first=, last=} (1-based inclusive)
    g.set(
        "__re_matches",
        lua.create_function(|lua, (pat, flags, subject, max): (String, i64, mlua::LuaString, Option<usize>)| {
            let re = build(&pat, flags)?;
            let bytes = subject.as_bytes();
            let text = String::from_utf8_lossy(&bytes).to_string();
            let out = lua.create_table()?;
            for (n, caps) in re.captures_iter(&text).enumerate() {
                if max.is_some_and(|m| m > 0 && n >= m) {
                    break;
                }
                let m = lua.create_table()?;
                for i in 0..caps.len() {
                    let c = lua.create_table()?;
                    match caps.get(i) {
                        Some(x) => {
                            c.set("str", x.as_str())?;
                            c.set("first", x.start() + 1)?;
                            c.set("last", x.end())?;
                        }
                        None => {
                            c.set("str", "")?;
                            c.set("first", 0)?;
                            c.set("last", -1)?;
                        }
                    }
                    m.raw_set(i + 1, c)?;
                }
                out.raw_set(n + 1, m)?;
            }
            Ok(out)
        })?,
    )?;

    // __re_sub(pattern, flags, subject, replacement, count) -> string
    g.set(
        "__re_sub",
        lua.create_function(|_, (pat, flags, subject, repl, count): (String, i64, mlua::LuaString, mlua::LuaString, Option<usize>)| {
            let re = build(&pat, flags)?;
            let text = String::from_utf8_lossy(&subject.as_bytes()).to_string();
            // Accept both $1 and \1 group references.
            let rep = String::from_utf8_lossy(&repl.as_bytes()).to_string();
            let rep = regex::Regex::new(r"\\(\d)").unwrap().replace_all(&rep, "$${$1}").to_string();
            let n = count.unwrap_or(0);
            Ok(re.replacen(&text, n, rep.as_str()).to_string())
        })?,
    )?;

    // __re_split(pattern, flags, subject, skip_empty, max_splits) -> { strings }
    g.set(
        "__re_split",
        lua.create_function(|lua, (pat, flags, subject, skip_empty, max): (String, i64, mlua::LuaString, Option<bool>, Option<usize>)| {
            let re = build(&pat, flags)?;
            let text = String::from_utf8_lossy(&subject.as_bytes()).to_string();
            let t: Table = lua.create_table()?;
            let mut last = 0;
            let mut n = 0;
            let skip = skip_empty.unwrap_or(false);
            for m in re.find_iter(&text) {
                if max.is_some_and(|x| x > 0 && n >= x) {
                    break;
                }
                let part = &text[last..m.start()];
                if !(skip && part.is_empty()) {
                    t.raw_set(t.raw_len() + 1, part)?;
                    n += 1;
                }
                last = m.end();
            }
            let tail = &text[last..];
            if !(skip && tail.is_empty()) {
                t.raw_set(t.raw_len() + 1, tail)?;
            }
            Ok(t)
        })?,
    )?;
    Ok(())
}
