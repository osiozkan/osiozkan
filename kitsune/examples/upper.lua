script_name = "Büyük harf + karaskel"
script_description = "Aegisub uyumluluk testi"
script_author = "kitsune"
script_version = "1"
include("karaskel.lua")
local util = require "util"

function upper(subs, sel, active)
  local meta, styles = karaskel.collect_head(subs, false)
  local res = aegisub.dialog.display({{class="label",label="x"},{class="edit",name="suffix",value="!"}}, {"Tamam","İptal"})
  for _, i in ipairs(sel) do
    local l = subs[i]
    karaskel.preproc_line(subs, meta, styles, l)
    l.text = l.text:upper() .. " [" .. l.kara.n .. "/" .. math.floor(l.width) .. "]"
    subs[i] = l
  end
  subs.append({class="dialogue", comment=false, layer=0, start_time=5000, end_time=6000, style="Default", actor="", margin_l=0, margin_r=0, margin_t=0, effect="", text=util.ass_color(1,2,3)})
  aegisub.debug.out("bitti %d satir", #sel)
  return sel
end
aegisub.register_macro("Upper", "büyük harf", upper)
