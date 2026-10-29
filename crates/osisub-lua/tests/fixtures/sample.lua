script_name = "Test makrosu"
script_description = "Aegisub uyumluluk testi"
script_author = "test"
script_version = "1"

include("karaskel.lua")
local re = require "re"
local unicode = require "unicode"

function shift_all(subs, sel, active)
  for _, i in ipairs(sel) do
    local l = subs[i]
    l.start_time = l.start_time + 1000
    l.end_time = l.end_time + 1000
    l.text = l.text .. "!"
    subs[i] = l
  end
  aegisub.set_undo_point("kaydir")
  return sel
end

function kara(subs, sel)
  local meta, styles = karaskel.collect_head(subs)
  local out = {}
  for _, i in ipairs(sel) do
    local l = subs[i]
    karaskel.preproc_line(subs, meta, styles, l)
    aegisub.log("sylls=%d first=%q dur=%d\n", l.kara.n, l.kara[1].text_stripped, l.kara[1].duration)
    local m = re.match(l.text_stripped, "(\\w)(\\w)")
    aegisub.log("re=%s,%s up=%s len=%d\n", m[2].str, m[3].str, unicode.tr_upper("iş"), unicode.len("şğı"))
    l.text = re.sub(l.text, "\\{[^}]*\\}", "")
    subs[i] = l
  end
  subs.append({class="dialogue", text="yeni", start_time=0, end_time=10, style="Default", layer=0, comment=false, actor="", effect="", margin_l=0, margin_r=0, margin_t=0, margin_b=0, extra={}})
end

function dlg(subs)
  local b, r = aegisub.dialog.display({{class="intedit", name="n", value=7}, {class="checkbox", name="c", value=true}})
  assert(b == true and r.n == 7 and r.c == true)
  aegisub.cancel()
end

aegisub.register_macro("Kaydir", "1sn kaydır", shift_all)
aegisub.register_macro("Karaoke", "kara", kara, function() return true end)
aegisub.register_macro("Iptal", "cancel", dlg)
