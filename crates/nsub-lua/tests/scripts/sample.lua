script_name = "Sample"
script_description = "test macro"
script_author = "t"
script_version = "1"

include("utils.lua")
include("karaskel.lua")
local re = require("re")

local function upper_all(subs, sel, active)
  for _, i in ipairs(sel) do
    local l = subs[i]
    l.text = l.text:upper()
    subs[i] = l
  end
  aegisub.set_undo_point("upper")
  return sel
end

local function karaoke(subs, sel)
  local meta, styles = karaskel.collect_head(subs, false)
  for _, i in ipairs(sel) do
    local l = subs[i]
    karaskel.preproc_line(subs, meta, styles, l)
    aegisub.log("syls=%d width>0=%s\n", l.kara.n, tostring(l.width > 0))
    l.effect = "k" .. l.kara.n
    l.actor = ass_color(255, 128, 0)
    subs[i] = l
  end
end

local function dup_and_delete(subs, sel)
  local first = sel[1]
  local l = subs[first]
  subs.insert(first + 1, l)
  subs.delete(first)
  local nn = re.sub(l.text, "(\\w+)", "<\\1>")
  subs.append({ class = "dialogue", text = nn, start_time = 0, end_time = 1000, style = "Default" })
  return { first }
end

local function cancel(subs) aegisub.cancel() end
local function dialog(subs, sel)
  local btn, res = aegisub.dialog.display({ { class = "edit", name = "word", value = "abc" }, { class = "checkbox", name = "flag", value = true } })
  aegisub.log("%s %s %s", tostring(btn), res.word, tostring(res.flag))
end

aegisub.register_macro("Test/Upper", "upper", upper_all)
aegisub.register_macro("Test/Karaoke", "kara", karaoke)
aegisub.register_macro("Test/Dup", "dup", dup_and_delete)
aegisub.register_macro("Test/Cancel", "cancel", cancel)
aegisub.register_macro("Test/Dialog", "dialog", dialog)
