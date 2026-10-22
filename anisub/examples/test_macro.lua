script_name = "Test macro"
script_description = "Exercises the Aegisub API shim"
script_author = "anisub"
script_version = "1"
include("karaskel.lua")

function run(subs, sel, active)
  local meta, styles = karaskel.collect_head(subs)
  local n = 0
  for i = 1, #subs do
    local l = subs[i]
    if l.class == "dialogue" then
      karaskel.preproc_line(subs, meta, styles, l)
      aegisub.log("line %d: %d syls, x=%.1f y=%.1f w=%.1f\n", i, l.kara.n, l.x, l.y, l.width)
      l.text = l.text .. "{\\fscx" .. math.round(l.width) .. "}"
      l.effect = "karaoke"
      subs[i] = l
      n = n + 1
    end
  end
  subs.append({ class = "dialogue", comment = true, layer = 0, start_time = 0, end_time = 1000, style = "Default", actor = "", margin_l = 0, margin_r = 0, margin_t = 0, margin_b = 0, effect = "", text = "added " .. n, section = "[Events]" })
  for i, l in ipairs(subs) do if l.class == "style" then aegisub.log("style %s\n", l.name) end end
  return sel
end
aegisub.register_macro("Test", "desc", run)
