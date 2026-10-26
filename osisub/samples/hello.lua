script_name = "Hello"
script_description = "smoke test"
script_author = "osisub"
script_version = "1"
include("karaskel.lua")

function shift_and_tag(subs, sel, act)
  local meta, styles = karaskel.collect_head(subs)
  for _, i in ipairs(sel) do
    local l = subs[i]
    karaskel.preproc_line(subs, meta, styles, l)
    l.start_time = l.start_time + 1000
    l.text = string.format("{\\pos(%d,%d)}", meta.res_x / 2, 900) .. l.text .. " " .. (l.width > 0 and "w" or "")
    subs[i] = l
  end
  subs.append({class="dialogue", comment=true, layer=0, start_time=0, end_time=1, style="Default", actor="", effect="", margin_l=0, margin_r=0, margin_t=0, text="added"})
  aegisub.log("done %d lines\n", #sel)
  return sel
end
aegisub.register_macro("Shift and tag", "shift +1s", shift_and_tag)
