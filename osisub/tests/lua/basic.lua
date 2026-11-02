script_name = "Test suite macros"
script_description = "Exercises the Aegisub API surface"
script_author = "osisub"
script_version = "1"

include("karaskel.lua")
local util = require("aegisub.util")

local function upper(subs, sel, active)
  for _, i in ipairs(sel) do
    local l = subs[i]
    l.text = l.text:upper()
    subs[i] = l
  end
  return sel
end

local function ins_del(subs, sel)
  local l = subs[sel[1]]
  l.text = "inserted"
  subs.insert(sel[1] + 1, l)
  subs.append({class = "dialogue", start_time = 9000, end_time = 9500, text = "appended", style = "Default"})
  subs.delete(sel[#sel] + 1)
  return {sel[1]}
end

local function kara(subs, sel)
  local meta, styles = karaskel.collect_head(subs)
  aegisub.log("res %dx%d styles=%d\n", meta.res_x, meta.res_y, styles.n)
  for _, i in ipairs(sel) do
    local l = subs[i]
    karaskel.preproc_line(subs, meta, styles, l)
    aegisub.log("line %d: stripped=%s kara=%d valign=%s x=%d\n", i, l.text_stripped, l.kara and l.kara.n or 0, l.valign, l.x)
    if l.kara then
      for k = 1, l.kara.n do aegisub.log(" syl %d '%s' %d..%d\n", k, l.kara[k].text_stripped, l.kara[k].start_time, l.kara[k].end_time) end
    end
  end
end

local function boom(subs, sel)
  local l = subs[sel[1]]
  l.text = "CHANGED"
  subs[sel[1]] = l
  error("kaboom")
end

local function cancel(subs, sel)
  subs.delete(sel[1])
  aegisub.cancel()
end

local function colors()
  assert(util.ass_color(255, 128, 0) == "&H0080FF&")
  local r, g, b, a = util.extract_color("&H00FF8000&")
  assert(r == 0 and g == 128 and b == 255 and a == 0, r .. "," .. g .. "," .. b)
  assert(util.interpolate_color(0.5, "&H000000&", "&HFFFFFF&") == "&H808080&")
  local r2, g2, b2 = util.HSV_to_RGB(120, 1, 1)
  assert(r2 == 0 and g2 == 255 and b2 == 0)
  assert(unicode.len("çağ") == 3)
  assert(unicode.to_upper_case("çağ") == "ÇAĞ")
  local t = {}
  for c in unicode.chars("aé") do t[#t + 1] = c end
  assert(#t == 2)
  local h, tl = util.headtail("hello big world")
  assert(h == "hello" and tl == "big world")
  aegisub.log("util ok\n")
end

local function dlg(subs)
  local ok, res = aegisub.dialog.display({
    {class = "intedit", name = "amount", value = 3},
    {class = "checkbox", name = "flag", value = true},
    {class = "dropdown", name = "mode", items = {"a", "b"}, value = "b"},
  }, {"Apply", "Cancel"}, {ok = "Apply", cancel = "Cancel"})
  aegisub.log("dlg %s %s %s %s\n", tostring(ok), tostring(res.amount), tostring(res.flag), res.mode)
end

local function frames()
  aegisub.log("frame %s ms %s\n", tostring(aegisub.frame_from_ms(1000)), tostring(aegisub.ms_from_frame(24)))
end

local function validate(subs, sel) return #sel > 1 end

aegisub.register_macro("Upper", "uppercases", upper)
aegisub.register_macro("InsDel", "insert/delete", ins_del)
aegisub.register_macro("Kara", "karaskel", kara)
aegisub.register_macro("Boom", "errors after editing", boom)
aegisub.register_macro("Cancel", "cancels", cancel)
aegisub.register_macro("Colors", "util checks", colors)
aegisub.register_macro("Dlg", "dialog defaults", dlg)
aegisub.register_macro("Frames", "frames", frames)
aegisub.register_macro("NeedsTwo", "validate", upper, validate)
aegisub.register_filter("Filt", "a filter", 50, function(subs, cfg)
  for i = 1, #subs do
    local l = subs[i]
    if l.class == "dialogue" then l.effect = "filtered"; subs[i] = l end
  end
end)
