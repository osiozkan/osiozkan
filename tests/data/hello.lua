script_name = "Test makroları"
include("util.lua")
include("karaskel.lua")

local function upper_all(subs, sel)
  for _, i in ipairs(sel) do
    local l = subs[i]
    l.text = l.text:upper()
    subs[i] = l
  end
  return sel
end
local function add_line(subs, sel)
  local l = subs[sel[1]]
  l.text = "eklendi"; l.start_time = 9000; l.end_time = 10000
  subs.append(l)
end
local function kara(subs, sel)
  local meta, styles = karaskel.collect_head(subs)
  local out = {}
  for _, i in ipairs(sel) do
    local l = subs[i]
    karaskel.preproc_line(subs, meta, styles, l)
    l.effect = "syl=" .. l.kara.n .. ";w=" .. math.floor(l.width)
    subs[i] = l
  end
end
local function dlg(subs, sel)
  local b, r = aegisub.dialog.display({{class="edit", name="x", value="42"}}, {"Tamam", "İptal"})
  local l = subs[sel[1]]; l.actor = b .. ":" .. r.x; subs[sel[1]] = l
end
local re = require("re")
local function rx(subs, sel)
  local l = subs[sel[1]]; l.text = re.sub(l.text, "(\\w+), (\\w+)", "\\2 \\1"); subs[sel[1]] = l
end
local function del(subs, sel) subs.delete(sel[1]) end
aegisub.register_macro("Büyük harf", "hepsini büyüt", upper_all)
aegisub.register_macro("Ekle", "satır ekle", add_line)
aegisub.register_macro("Karaoke", "karaskel", kara)
aegisub.register_macro("Diyalog", "dialog", dlg)
aegisub.register_macro("Regex", "re", rx)
aegisub.register_macro("Sil", "sil", del)
