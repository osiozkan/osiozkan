script_name = "Kitsune örnek makrolar"
script_description = "Aegisub API uyumluluk örneği"
script_author = "kitsune"
script_version = "1"

function strip_tags_macro(subs, sel)
  for _, i in ipairs(sel) do
    local l = subs[i]
    l.text = l.text:gsub("%b{}", "")
    subs[i] = l
  end
  return sel
end

function shift_macro(subs, sel)
  local btn, res = aegisub.dialog.display({
    {class="label", label="Kaydırma (ms):", x=0, y=0, width=1, height=1},
    {class="intedit", name="ms", value=500, x=1, y=0, width=1, height=1},
  }, {"Uygula", "İptal"})
  if btn ~= "Uygula" then aegisub.cancel() end
  for _, i in ipairs(sel) do
    local l = subs[i]
    l.start_time = l.start_time + res.ms; l.end_time = l.end_time + res.ms
    subs[i] = l
  end
  return sel
end

aegisub.register_macro("Kitsune/Etiketleri temizle", "Seçili satırlardaki {} bloklarını siler", strip_tags_macro)
aegisub.register_macro("Kitsune/Zamanı kaydır", "Seçili satırları kaydırır", shift_macro)
