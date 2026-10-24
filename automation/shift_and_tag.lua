script_name = "OsiSub örnek: gecikme + etiket"
script_description = "Seçili satırları 500ms kaydırır ve karaskel ile genişlik hesaplar"
script_author = "osiozkan"
script_version = "1"

include("karaskel.lua")

function shift_sel(subs, sel, active)
  local meta, styles = karaskel.collect_head(subs, false)
  for _, i in ipairs(sel) do
    local l = subs[i]
    karaskel.preproc_line(subs, meta, styles, l)
    l.start_time = l.start_time + 500
    l.end_time = l.end_time + 500
    l.effect = string.format("w=%d", math.floor(l.width))
    subs[i] = l
  end
  aegisub.debug.out("%d satır kaydırıldı", #sel)
  aegisub.set_undo_point(script_name)
  return sel
end

aegisub.register_macro("OsiSub/500ms kaydır", "Seçili satırlar +500ms", shift_sel)
