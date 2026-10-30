script_name = "Örnek: İtalik Yap ve Süre Ekle"
script_description = "Seçili satırlara {\\i1} ekler, bitiş zamanını 200ms uzatır"
script_author = "kitsune"
script_version = "1.0"

include("karaskel.lua")

function ornek(subs, sel, active)
  local meta, styles = karaskel.collect_head(subs, false)
  for _, i in ipairs(sel) do
    local l = subs[i]
    karaskel.preproc_line(subs, meta, styles, l)
    l.text = "{\\i1}" .. l.text_stripped .. "{\\i0}"
    l.end_time = l.end_time + 200
    subs[i] = l
  end
  subs.append({class="dialogue", start_time=0, end_time=1000, style="Default", text="eklendi", layer=0, actor="", effect="", comment=false, margin_l=0, margin_r=0, margin_t=0})
  aegisub.debug.out(0, "%d satır işlendi, %d toplam\n", #sel, #subs)
  return sel
end

function dogrula(subs, sel) return #sel > 0 end

aegisub.register_macro(script_name, script_description, ornek, dogrula)
