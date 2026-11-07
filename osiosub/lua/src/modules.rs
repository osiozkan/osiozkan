//! Bundled Lua modules: util, karaskel, utf8 (Lua), unicode and re (Rust).
use mlua::{Function, Lua, Result, Table, Value};
use regex::RegexBuilder;

const UTIL: &str = include_str!("../lib/util.lua");
const UTF8: &str = include_str!("../lib/utf8.lua");

/// Unmodified Aegisub include files, resolved by `include()` when not found next to the script.
pub fn embedded_include(name: &str) -> Option<&'static str> {
    Some(match name {
        "karaskel.lua" => include_str!("../lib/include/karaskel.lua"),
        "karaskel-auto4.lua" => include_str!("../lib/include/karaskel-auto4.lua"),
        "utils.lua" => include_str!("../lib/include/utils.lua"),
        "utils-auto4.lua" => include_str!("../lib/include/utils-auto4.lua"),
        "unicode.lua" => include_str!("../lib/include/unicode.lua"),
        "clipboard.lua" => include_str!("../lib/include/clipboard.lua"),
        "re.lua" => include_str!("../lib/include/re.lua"),
        "cleantags.lua" => include_str!("../lib/include/cleantags.lua"),
        _ => return None,
    })
}

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
    preload(lua, "aegisub.util", "return require('util')")?;
    preload(lua, "utf8", UTF8)?;
    preload(lua, "aegisub.utf8", "return require('utf8')")?;

    // ---- unicode ----
    let u = lua.create_table()?;
    u.set("len", lua.create_function(|_, s: mlua::String| Ok(String::from_utf8_lossy(&s.as_bytes()).chars().count()))?)?;
    u.set("to_upper", lua.create_function(|_, s: String| Ok(s.to_uppercase()))?)?;
    u.set("to_lower", lua.create_function(|_, s: String| Ok(s.to_lowercase()))?)?;
    u.set("to_fold_case", lua.create_function(|_, s: String| Ok(s.to_lowercase()))?)?;
    u.set("codepoint", lua.create_function(|_, (s, pos): (mlua::String, Option<usize>)| {
        let b = s.as_bytes();
        let p = pos.unwrap_or(1).max(1) - 1;
        Ok(String::from_utf8_lossy(b.get(p..).unwrap_or(&[])).chars().next().map(|c| c as u32).unwrap_or(0))
    })?)?;
    // number of bytes of the UTF-8 character starting at byte position `pos` (1-based)
    u.set("charwidth", lua.create_function(|_, (s, pos): (mlua::String, Option<usize>)| {
        let b = s.as_bytes();
        let p = pos.unwrap_or(1).max(1) - 1;
        Ok(match b.get(p) { Some(&c) if c >= 0xF0 => 4, Some(&c) if c >= 0xE0 => 3, Some(&c) if c >= 0xC0 => 2, Some(_) => 1, None => 0 })
    })?)?;
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
    loaded.set("aegisub.unicode", lua.globals().get::<Value>("unicode")?)?;
    loaded.set("aegisub.re", lua.globals().get::<Value>("re")?)?;
    // clipboard module backed by aegisub.clipboard
    lua.load("package.loaded['aegisub.clipboard'] = aegisub.clipboard").exec()?;
    let _: Option<Function> = None;
    lua.load("utf8 = require('utf8')").exec()?;
    lua.load(r#"
-- aegisub.parse_karaoke_data(line): syllable table indexed from 0 (0 = text before the first \k tag)
function aegisub.parse_karaoke_data(line)
  local text, out, idx, t0 = line.text, {}, 0, 0
  local cur = { text = "", dur = 0, tag = "k" }
  local function push()
    local plain = cur.text:gsub("%b{}", "")
    local d = cur.dur * 10
    out[idx] = { duration = d, start_time = t0, end_time = t0 + d, tag = cur.tag, text = cur.text, text_stripped = plain }
    idx = idx + 1; t0 = t0 + d
  end
  local pos = 1
  while true do
    local bs, be = text:find("%b{}", pos)
    if not bs then cur.text = cur.text .. text:sub(pos); break end
    cur.text = cur.text .. text:sub(pos, bs - 1)
    local block = text:sub(bs, be)
    local a, b, tag, dur = block:find("\\([kK][fo]?)(%d+)")
    if a then
      push()
      local rest = block:sub(1, a - 1) .. block:sub(b + 1)
      cur = { text = (rest == "{}") and "" or rest, dur = tonumber(dur), tag = tag:lower() }
    else
      cur.text = cur.text .. block
    end
    pos = be + 1
  end
  push()
  return out
end
"#).exec()?;
    // Aegisub's `subs` honours ipairs (LuaJIT/5.2 semantics); stock Lua 5.1 does not.
    lua.load(r#"
local _ipairs = ipairs
function ipairs(t)
  if type(t) == "userdata" then
    return function(o, i) i = i + 1; if i <= #o then return i, o[i] end end, t, 0
  end
  return _ipairs(t)
end
"#).exec()?;
    Ok(())
}
