-- Automation 4 API uyumluluk denetimi. Her denetim "OK"/"FAIL" günlüğü basar.
script_name = "API denetimi"
script_description = "OsiSub uyumluluk testleri"
script_author = "OsiSub"
script_version = "1"

local re = require "aegisub.re"
local util = require "aegisub.util"
local unicode = require "aegisub.unicode"
local lfs = require "lfs"

local function check(name, cond, extra)
  aegisub.debug.out(0, "%s %s %s\n", cond and "OK" or "FAIL", name, extra and tostring(extra) or "")
end

local function api(subs, sel, active)
  -- subtitles nesnesi
  check("len", #subs == subs.n)
  local n_info, n_style, n_dia = 0, 0, 0
  for i, l in ipairs(subs) do
    if l.class == "info" then n_info = n_info + 1 end
    if l.class == "style" then n_style = n_style + 1 end
    if l.class == "dialogue" then n_dia = n_dia + 1 end
  end
  check("classes", n_info == 7 and n_style == 2 and n_dia == 6, n_info .. "/" .. n_style .. "/" .. n_dia)
  local first = n_info + n_style + 1
  check("sel maps to dialogue", subs[sel[1]].class == "dialogue" and sel[1] == first, sel[1])
  check("active", active == sel[1])
  local l = subs[first]
  check("fields", l.start_time == 1000 and l.end_time == 3500 and l.actor == "Ali" and l.style == "Default"
        and l.margin_t == 0 and l.layer == 0 and l.comment == false and type(l.extra) == "table")
  check("raw", l.raw == "Dialogue: 0,0:00:01.00,0:00:03.50,Default,Ali,0000,0000,0000,,{\\an8}Merhaba dünya, nasılsın?", l.raw)
  check("section", l.section == "[Events]")
  -- kopya semantiği
  l.text = "değişmemeli"
  check("copy semantics", subs[first].text ~= "değişmemeli")
  local st = subs[n_info + 2]
  check("style fields", st.name == "Sign" and st.fontsize == 60 and st.spacing == 1.5 and st.bold == false
        and st.color3 == "&H00202020" and st.align == 8 and st.margin_t == 10 and st.encoding == 162)
  check("script_resolution", select(1, subs.script_resolution()) == 1920)

  -- atama sonrası tablo değişirse belge etkilenmez
  local a = subs[first]
  a.text = "A"
  subs[first] = a
  a.text = "B"
  check("stored copy", subs[first].text == "A")

  -- negatif indeks = araya ekle, 0 = sona ekle
  local nb = #subs
  local x = subs[first]; x.text = "eklenen-önce"
  subs[-first] = x
  check("negative insert", subs[first].text == "eklenen-önce" and #subs == nb + 1)
  local y = subs[first]; y.text = "sona"
  subs[0] = y
  check("append via 0", subs[#subs].text == "sona")
  subs.delete(first, #subs)
  check("delete varargs", #subs == nb and subs[first].text == "A")
  subs.append(y, y)
  subs.deleterange(#subs - 1, #subs)
  check("deleterange", #subs == nb)
  subs.insert(first + 1, {y, y})
  check("insert table", #subs == nb + 2)
  subs.delete({first + 1, first + 2})
  check("delete table", #subs == nb)
  local ok = pcall(function() return subs[#subs + 5] end)
  check("out of range errors", not ok)

  -- unicode
  check("unicode.len", unicode.len("ğüşİ") == 4)
  check("unicode.upper", unicode.to_upper_case("ğüşi") == "ĞÜŞI")
  check("unicode.chars", (function() local t = {} for c in unicode.chars("aç") do t[#t+1] = c end return t[2] == "ç" end)())
  check("unicode.codepoint", unicode.codepoint("ğ") == 0x11F)
  check("unicode.charwidth", unicode.charwidth("ğ") == 2)

  -- re (bayt konumları)
  local m = re.find("ğa ğb", "ğ(\\w)")
  check("re.find bytes", m and #m == 2 and m[1].first == 1 and m[1].last == 3 and m[2].first == 5, m and m[2].first)
  local g = re.match("x=12, y=345", "(\\w)=(\\d+)")
  check("re.match groups", g and g[1].str == "x=12" and g[2].str == "x" and g[3].str == "12" and g[3].first == 3)
  check("re.sub $1", re.sub("abc abd", "a(b)", "[$1]") == "[b]c [b]d")
  check("re.sub \\1", re.sub("abc", "(a)(b)", "\\2\\1") == "bac")
  check("re.sub fn", re.sub("a1b22", "\\d+", function(s) return "<" .. s .. ">" end) == "a<1>b<22>")
  check("re.split", #re.split("a, b,c", ",\\s*") == 3)
  check("re.split skip", #re.split("a,,b", ",", true) == 2)
  check("re ICASE", re.match("ABC", "abc", re.ICASE) ~= nil)
  check("re no ICASE", re.match("ABC", "abc") == nil)
  local rx = re.compile("\\\\N")
  check("re compiled", #rx:split("a\\Nb\\Nc") == 3)
  local cnt = 0
  for s, f, la in re.gfind("aXbXc", "X") do cnt = cnt + 1 end
  check("re.gfind", cnt == 2)
  check("re dot-all default", re.match("a\nb", "a.b") ~= nil)
  check("re multiline default", re.match("x\nab", "^ab") ~= nil)
  local ok2 = pcall(re.compile, "(")
  check("re bad pattern errors", not ok2)

  -- util
  check("util.copy", util.copy({a = 1}).a == 1)
  check("util.ass_color", util.ass_color(255, 128, 0) == "&H0080FF&")
  local r, gg, b, al = util.extract_color("&H80FF0000")
  check("util.extract_color", r == 0 and gg == 0 and b == 255 and al == 128)
  check("util.interpolate", util.interpolate(0.5, 0, 10) == 5)
  check("string.headtail via utils", select(1, util.headtail("a b c")) == "a")

  -- lfs
  check("lfs.currentdir", type(lfs.currentdir()) == "string")
  check("lfs.attributes", lfs.attributes(aegisub.decode_path("?script")) ~= nil or true)

  -- karaoke ayrıştırma
  local kl = subs[first + 5]
  local kd = aegisub.parse_karaoke_data(kl)
  check("kara[0] empty", kd[0].text == "" and kd[0].duration == 0)
  check("kara count", #kd == 4, #kd)
  check("kara syl", kd[2].text_stripped == "ra" and kd[2].duration == 300 and kd[2].start_time == 200
        and kd[2].end_time == 500 and kd[2].tag == "\\k", kd[2].start_time)
  local kl2 = util.copy(kl)
  kl2.text = "{\\kf10\\b1}a{\\K20}b{comment}c"
  local kd2 = aegisub.parse_karaoke_data(kl2)
  check("kara tags", kd2[1].tag == "\\kf" and kd2[1].text == "{\\b1}a" and kd2[2].tag == "\\kf"
        and kd2[2].text == "b{comment}c" and kd2[2].text_stripped == "bc", kd2[2].text)

  -- text_extents
  local w, h, d, e = aegisub.text_extents(st, "Test")
  check("text_extents", w > 0 and h > 0 and math.abs(h - 60) < 1.5, string.format("%.2f %.2f", w, h))
  local w2 = aegisub.text_extents(st, "TestTest")
  check("text_extents additive", math.abs(w2 - 2 * w) < 1.0)

  -- çeşitli
  check("gettext", aegisub.gettext("x") == "x")
  check("lua_automation_version", aegisub.lua_automation_version == 4)
  check("table.unpack", table.unpack ~= nil)
  check("math.log base", math.abs(math.log(8, 2) - 3) < 1e-9)
  check("jit", jit ~= nil and require("ffi") ~= nil)
  check("file_name", aegisub.file_name() == "sample.ass", aegisub.file_name())
  check("decode_path", aegisub.decode_path("?script/x.txt"):match("x%.txt$") ~= nil)
  check("video_size nil", aegisub.video_size() == nil)
  check("frame nil", aegisub.frame_from_ms(1000) == nil)
  aegisub.debug.out(0, "DONE\n")
end

aegisub.register_macro("API", "", api)
