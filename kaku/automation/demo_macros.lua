script_name = "Kaku demo"
script_description = "Exercises the Aegisub-compatible API"
script_author = "kaku"
script_version = "1"

include("karaskel.lua")
local re = require "re"
local util = require "util"

local function uppercase(subs, sel)
  for _, i in ipairs(sel) do
    local l = subs[i]
    if l.class == "dialogue" then
      l.text = l.text:upper()
      subs[i] = l
    end
  end
  return sel
end

local function kara_fx(subs, sel)
  local meta, styles = karaskel.collect_head(subs, false)
  local n = 0
  for i = #subs, 1, -1 do
    local l = subs[i]
    if l.class == "dialogue" and l.text:find("\\k") then
      karaskel.preproc_line(subs, meta, styles, l)
      for s = 1, l.kara.n do
        local syl = l.kara[s]
        local nl = util.copy(l)
        nl.layer = 1
        nl.start_time = l.start_time + syl.start_time
        nl.end_time = l.start_time + syl.end_time
        nl.text = string.format("{\\pos(%d,%d)}%s", syl.x, syl.y, syl.text_stripped)
        subs.insert(i + s, nl)
        n = n + 1
      end
      l.comment = true
      subs[i] = l
    end
  end
  aegisub.debug.out("generated %d syllable lines", n)
end

local function dialog_macro(subs, sel)
  local btn, res = aegisub.dialog.display({
    { class = "label", label = "Suffix", x = 0, y = 0 },
    { class = "edit", name = "suffix", value = "!", x = 1, y = 0 },
    { class = "checkbox", name = "dbl", label = "double", value = false, x = 0, y = 1 },
  }, { "Apply", "Cancel" })
  if btn ~= "Apply" then aegisub.cancel() end
  for _, i in ipairs(sel) do
    local l = subs[i]
    if l.class == "dialogue" then
      l.text = l.text .. res.suffix .. (res.dbl and res.suffix or "")
      subs[i] = l
    end
  end
end

local function regex_macro(subs, sel)
  for i = 1, #subs do
    local l = subs[i]
    if l.class == "dialogue" then
      l.text = re.sub(l.text, "\\s+(!)", "$1")
      subs[i] = l
    end
  end
end

aegisub.register_macro("Uppercase", "Uppercases text", uppercase)
aegisub.register_macro("Kara FX", "Explode syllables", kara_fx)
aegisub.register_macro("Dialog", "Uses dialog", dialog_macro)
aegisub.register_macro("Regex", "Uses re", regex_macro)
