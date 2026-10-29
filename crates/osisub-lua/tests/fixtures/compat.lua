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
  check(util.interpolate_color(0.5, "&H000000&", "&HFFFFFF&") == "&H808080&", "interpolate_color")
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
  check(ok == "Tamam" and res.dd == "b" and res.col == "&HFF0000&", "dialog.display")
  aegisub.set_undo_point("uyumluluk")
  return { first }
end

aegisub.register_macro("Uyumluluk testi", "tüm kalıplar", all_checks, function(subs, sel) return #sel > 0, "seçim gerekli" end)
aegisub.register_filter("Filtre", "dışa aktarım filtresi", 50, function(subs, config) for i = 1, #subs do if subs[i].class == "dialogue" then local l = subs[i]; l.text = l.text .. "?"; subs[i] = l end end end)
