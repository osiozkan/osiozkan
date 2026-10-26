script_name = "Dialog demo"
script_description = "asks for a time offset and a prefix"
script_author = "osisub"
script_version = "1"

function offset_macro(subs, sel)
  local btn, r = aegisub.dialog.display({
    {class="label", label="Kaydırma (ms):", x=0, y=0, width=2, height=1},
    {class="intedit", name="ms", value=500, min=-60000, max=60000, x=2, y=0, width=2, height=1},
    {class="label", label="Önek:", x=0, y=1, width=2, height=1},
    {class="edit", name="prefix", value="", x=2, y=1, width=2, height=1},
    {class="checkbox", name="bold", label="Kalın yap", value=false, x=0, y=2, width=4, height=1},
  }, {"Uygula", "İptal"}, {ok="Uygula", cancel="İptal"})
  if btn ~= "Uygula" then aegisub.cancel() end
  for _, i in ipairs(sel) do
    local l = subs[i]
    l.start_time = l.start_time + r.ms
    l.end_time = l.end_time + r.ms
    l.text = (r.bold and "{\\b1}" or "") .. r.prefix .. l.text
    subs[i] = l
  end
  aegisub.log("%d satır kaydırıldı (%+d ms)\n", #sel, r.ms)
  return sel
end
aegisub.register_macro("Kaydır + önek", "dialog örneği", offset_macro)
