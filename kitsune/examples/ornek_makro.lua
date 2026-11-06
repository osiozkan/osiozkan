-- Kitsune örnek makrosu: seçili satırların sonuna kısa bir süre ekler ve etiketleri temizler.
script_name = "Örnek: Süre uzat"
script_description = "Seçili satırların bitişini 200 ms uzatır"
script_author = "kitsune"
script_version = "1"

include("utils.lua")

local function uzat(subs, sel, active)
  for _, i in ipairs(sel) do
    local l = subs[i]
    l.end_time = l.end_time + 200
    subs[i] = l
  end
  aegisub.debug.out("%d satır uzatıldı", #sel)
  return sel
end

aegisub.register_macro(script_name, script_description, uzat)
