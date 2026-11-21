script_name = "Uyumluluk"
script_description = "Yaygın Aegisub betik kalıpları"
script_author = "osisub"
script_version = "1.0"
script_namespace = "osisub.compat"

include("utils.lua")
include("karaskel.lua")
local re = require "aegisub.re"
local utf8 = require "utf8"
local unicode = require "aegisub.unicode"
local clipboard = require "aegisub.clipboard"
local lfs = require "lfs"
local ffi = require "ffi"
local bit = require "bit"
local util2 = require "aegisub.util"

local function check(c, m) if not c then error("başarısız: " .. m, 2) end end

function all_checks(subs, sel, active)
  -- util
  check(util.ass_color(255, 128, 0) == "&H0080FF&", "ass_color")
  check(util.ass_alpha(128) == "&H80&", "ass_alpha")
  check(util.ass_style_color(1, 2, 3, 4) == "&H04030201", "ass_style_color")
  local r, g, b, a = util.extract_color("&H80FF8040")
  check(r == 0x40 and g == 0x80 and b == 0xFF and a == 0x80, "extract_color")
  check(util.interpolate_color(0.5, "&H000000&", "&HFFFFFF&") == "&H7F7F7F&", "interpolate_color (Aegisub %02X keser)")
  check(util.interpolate(1.5, 0, 10) == 10 and util.interpolate(-1, 0, 10) == 0, "interpolate sınırlı")
  local r2, g2, b2, a2 = util.extract_color("&H80&")
  check(r2 == 0 and g2 == 0 and b2 == 0 and a2 == 128, "extract_color alfa")
  check(util.extract_color("zz") == nil, "extract_color eşleşmezse nil")
  local r3, g3, b3 = util.extract_color("#102030")
  check(r3 == 0x10 and g3 == 0x20 and b3 == 0x30, "extract_color #RRGGBB")
  local mt = setmetatable({ 1 }, { __index = function() return 5 end })
  check(getmetatable(util.deep_copy(mt)) == nil, "deep_copy metatablo kopyalamaz")
  check(unicode.to_upper_case("iğ") == "IĞ" and unicode.to_lower_case("AB") == "ab", "unicode.to_upper_case")
  -- re: split / gsplit / gmatch / gfind, boost \h
  local parts = re.split("a,b,,c", ",")
  check(#parts == 4 and parts[1] == "a" and parts[3] == "" and parts[4] == "c", "re.split")
  parts = re.split("a,b,,c", ",", true)
  check(#parts == 3, "re.split skip_empty")
  parts = re.split("a,b,c", ",", false, 1)
  check(#parts == 2 and parts[2] == "b,c", "re.split max_split")
  local acc = {}
  for p in re.gsplit("x y", " ") do acc[#acc + 1] = p end
  check(#acc == 2 and acc[2] == "y", "re.gsplit")
  acc = {}
  for m in re.gmatch("a1b22", "(\\d+)") do acc[#acc + 1] = m[2].str end
  check(#acc == 2 and acc[1] == "1" and acc[2] == "22", "re.gmatch")
  acc = {}
  for str, first, last in re.gfind("a1b22", "\\d+") do acc[#acc + 1] = str .. first .. last end
  check(acc[1] == "122" and acc[2] == "2245", "re.gfind")
  local rc = re.compile(",")
  check(#rc:split("1,2") == 2, "compiled:split")
  check(re.sub("a b\tc", "\\h", "_") == "a_b_c", "re \\h yatay boşluk")
  check(re.match("ABC", "b", re.ICASE) ~= nil, "re ICASE")
  check(util.interpolate(0.25, 0, 100) == 25, "interpolate")
  local h, s, v = util.RGB_to_HSV(255, 0, 0)
  check(h == 0 and s == 1 and v == 1, "RGB_to_HSV")
  local rr, gg, bb = util.HSV_to_RGB(120, 1, 1)
  check(rr == 0 and gg == 255 and bb == 0, "HSV_to_RGB")
  check(util2.clamp(5, 0, 3) == 3, "aegisub.util")
  check(("  x "):trim() == "x", "string.trim")
  local hd, tl = ("ab cd ef"):headtail()
  check(hd == "ab" and tl == "cd ef", "headtail")
  -- utf8 / unicode
  check(utf8.len("şğı") == 3 and utf8.sub("çalışma", 2, 4) == "alı", "utf8.sub")
  check(utf8.reverse("aşk") == "kşa" and utf8.char(0x15F) == "ş" and utf8.codepoint("ş") == 0x15F, "utf8.char")
  check(unicode.to_upper("abc") == "ABC" and unicode.len("日本語") == 3, "unicode")
  local n = 0
  for c in unicode.chars("aşk") do n = n + 1 end
  check(n == 3, "unicode.chars")
  -- re
  check(re.match("abc123", "(\\d+)")[2].str == "123", "re.match")
  check(re.sub("a-b-c", "-", "+") == "a+b+c", "re.sub")
  check(#re.find("a1b22c333", "\\d+") == 3, "re.find")
  local rx = re.compile("(\\w)(\\w)", re.ICASE)
  check(rx:match("xy")[3].str == "y" and rx:sub("ab cd", "$2$1") == "ba dc", "re.compile")
  -- ffi/bit
  check(bit.band(0xFF, 0x0F) == 0x0F and bit.bxor(1, 3) == 2, "bit")
  check(ffi.new("int[3]")[0] == 0, "ffi")
  -- lfs
  check(lfs.attributes("/", "mode") == "directory", "lfs")
  -- clipboard
  clipboard.set("merhaba")
  check(clipboard.get() == "merhaba", "clipboard")
  -- decode_path / props
  check(aegisub.decode_path("?temp/x"):find("x$"), "decode_path")
  local p = aegisub.project_properties()
  check(type(p) == "table" and p.video_position == 0, "project_properties")
  check(select("#", aegisub.video_size()) == 0, "video_size headless")
  check(aegisub.frame_from_ms(1000) == nil, "frame_from_ms headless")
  -- text_extents
  local meta, styles = karaskel.collect_head(subs)
  check(meta.res_x == 1280 and meta.res_y == 720 and styles.n == 1, "collect_head")
  local w, h2, d, e = aegisub.text_extents(styles[1], "Merhaba")
  check(w > 0 and h2 > 0 and d >= 0, "text_extents")
  -- subs API
  local first
  for i = 1, #subs do if subs[i].class == "dialogue" then first = i break end end
  local l = subs[first]
  check(l.section == "[Events]" and l.extra ~= nil and l.margin_t == l.margin_b, "line fields")
  subs.insert(first, util.deep_copy(l))
  check(subs[first + 1].text == l.text, "insert")
  subs.delete(first)
  check(subs[first].text == l.text, "delete")
  subs.deleterange(first, first)
  subs.append(l)
  subs[0] = l
  local before = #subs
  local marker = util.deep_copy(l); marker.text = "NEG"
  subs[-first] = marker
  check(#subs == before + 1 and subs[first].text == "NEG", "negatif dizin ekleme")
  subs.delete(first)
  check(subs.n == #subs, "subs.n")
  local cnt = 0
  for i, line in ipairs(subs) do cnt = cnt + 1 end
  check(cnt == #subs, "ipairs(subs)")
  aegisub.progress.set(50)
  aegisub.progress.task("görev")
  aegisub.progress.title("başlık")
  aegisub.log(2, "düzey %d\n", 2)
  aegisub.log("100%% güvenli\n")
  local ok, res = aegisub.dialog.display({ { class = "label", label = "Merhaba", x = 0, y = 0 }, { class = "dropdown", name = "dd", items = { "a", "b" }, value = "b", x = 0, y = 1 }, { class = "color", name = "col", value = "&HFF0000&" } }, { "Tamam", "Vazgeç" }, { ok = "Tamam", cancel = "Vazgeç" })
  check(ok == "Tamam" and res.dd == "b" and res.col == "#0000FF", "dialog.display (renk #RRGGBB) " .. tostring(res.col))
  -- osisub'a özgü API
  local os_ = require "osisub"
  check(os_.tags.clean("{\\fs20\\fs30}a{\\i1}b{\\i1}c") == "{\\fs30}a{\\i1}bc", "osisub.tags.clean")
  check(os_.text.upper("istanbul ılık") == "İSTANBUL ILIK", "osisub.text.upper (Türkçe)")
  check(os_.text.upper("istanbul", false) == "ISTANBUL", "osisub.text.upper (tr kapalı)")
  local fixed, n = os_.text.tr_fix("herşey yada hiçbirşey")
  check(fixed == "her şey ya da hiçbir şey" and n == 3, "osisub.text.tr_fix " .. tostring(fixed))
  check(os_.romaji_to_kana("kimi no na wa") == "きみのなわ", "osisub.romaji_to_kana")
  check(os_.furigana.strip("{\\k20}君|きみ{\\k10}の") == "{\\k20}君{\\k10}の" and os_.furigana.has("{\\k5}a|b"), "osisub.furigana")
  check(os_.time.format(3723450) == "1:02:03.45" and os_.time.parse("0:00:01.50") == 1500, "osisub.time")
  check(os_.tags.visible("{\\b1}a{x}b") == "ab", "osisub.tags.visible")
  local lint = os_.tags.lint("{\\blurr2}a", 1000)
  check(#lint >= 1 and lint[1].kind == "error", "osisub.tags.lint")
  check(os_.text.balance("Bu çok uzun bir satır ve ikiye bölünmesi gerekiyor sanırım", 30):find("\\N") ~= nil, "osisub.text.balance")
  -- stil renkleri sonunda & ile; subs.delete(tablo)
  for i = 1, #subs do
    if subs[i].class == "style" then
      check(subs[i].color1:match("^&H%x%x%x%x%x%x%x%x&$") ~= nil, "stil rengi &HAABBGGRR& " .. subs[i].color1)
      local st = subs[i]; subs[i] = st
      break
    end
  end
  local n0 = #subs
  local l = subs[n0]
  subs.append(l, l)
  subs.delete({ n0 + 1, n0 + 2 })
  check(#subs == n0, "subs.delete(tablo)")
  aegisub.set_undo_point("uyumluluk")
  return { first }
end

aegisub.register_macro("Uyumluluk testi", "tüm kalıplar", all_checks, function(subs, sel) return #sel > 0, "seçim gerekli" end)
aegisub.register_filter("Filtre", "dışa aktarım filtresi", 50, function(subs, config) for i = 1, #subs do if subs[i].class == "dialogue" then local l = subs[i]; l.text = l.text .. "?"; subs[i] = l end end end)
