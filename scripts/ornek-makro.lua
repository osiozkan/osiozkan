script_name = "SubForge örnek makrolar"
script_description = "Auto4 uyumluluğunu denemek için örnekler"
script_author = "SubForge"
script_version = "1.0"

include("karaskel.lua")

-- 1) Seçili satırların başına/sonuna metin ekler (iletişim kutusu + geri al desteği)
local function ekle(subs, sel)
  local btn, cfg = aegisub.dialog.display({
    {class = "label", label = "Başa eklenecek:", x = 0, y = 0},
    {class = "edit", name = "bas", text = "", x = 1, y = 0, width = 2},
    {class = "label", label = "Sona eklenecek:", x = 0, y = 1},
    {class = "edit", name = "son", text = "", x = 1, y = 1, width = 2},
    {class = "checkbox", name = "yorum", label = "Yorum satırlarını atla", value = true, x = 0, y = 2, width = 3},
  }, {"Uygula", "İptal"}, {ok = "Uygula", cancel = "İptal"})
  if not btn then aegisub.cancel() end
  for n, i in ipairs(sel) do
    aegisub.progress.set(n / #sel * 100)
    local l = subs[i]
    if not (cfg.yorum and l.comment) then
      l.text = cfg.bas .. l.text .. cfg.son
      subs[i] = l
    end
  end
  aegisub.set_undo_point("Metin ekle")
  return sel
end

-- 2) karaskel ile her seçili satırın genişliğini/konumunu günlüğe yazar
local function olc(subs, sel)
  local meta, styles = karaskel.collect_head(subs, false)
  for _, i in ipairs(sel) do
    local l = subs[i]
    karaskel.preproc_line(subs, meta, styles, l)
    aegisub.log("#%d  genişlik=%.1f  x=%.1f  y=%.1f  hece=%d\n", i, l.width, l.x, l.y, l.kara.n)
  end
end

local function secim_var(subs, sel) return #sel > 0, "Önce satır seçin" end

aegisub.register_macro("SubForge/Metin ekle", "Seçili satırlara başa/sona metin ekler", ekle, secim_var)
aegisub.register_macro("SubForge/Satır ölçüleri", "karaskel ile ölçüleri günlüğe yazar", olc, secim_var)
