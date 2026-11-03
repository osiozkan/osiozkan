script_name = "Kaede Türkçe Araçları"
script_description = "Türkçe I/İ doğru büyük-küçük harf dönüşümleri ve temizlik makroları"
script_author = "Kaede"
script_version = "1.0"
script_namespace = "kaede.turkce"

local function each_selected(subs, sel, fn)
  for _, i in ipairs(sel) do
    local l = subs[i]
    if l.class == "dialogue" then l.text = fn(l.text); subs[i] = l end
  end
  aegisub.set_undo_point(script_name)
  return sel
end

aegisub.register_macro("Kaede/Türkçe/BÜYÜK HARF", "Seçili satırları Türkçe büyük harfe çevirir",
  function(subs, sel) return each_selected(subs, sel, function(t) return kaede.apply_case(t, "upper") end) end)
aegisub.register_macro("Kaede/Türkçe/küçük harf", "Seçili satırları Türkçe küçük harfe çevirir",
  function(subs, sel) return each_selected(subs, sel, function(t) return kaede.apply_case(t, "lower") end) end)
aegisub.register_macro("Kaede/Türkçe/Cümle düzeni", "İlk harf büyük, gerisi küçük (cümle başlarında büyük)",
  function(subs, sel) return each_selected(subs, sel, function(t) return kaede.apply_case(t, "sentence") end) end)
aegisub.register_macro("Kaede/Türkçe/Her Sözcük Baş Harfi", "Başlık düzeni",
  function(subs, sel) return each_selected(subs, sel, function(t) return kaede.apply_case(t, "title") end) end)
aegisub.register_macro("Kaede/Metin/Üç noktayı … yap", "'...' → '…'",
  function(subs, sel) return each_selected(subs, sel, function(t) return (t:gsub("%.%.%.", "…")) end) end)
aegisub.register_macro("Kaede/Metin/Satırları dengele", "Tek satırlık metni dengeli iki satıra böler",
  function(subs, sel)
    local b, r = aegisub.dialog.display({
      { class = "label", label = "Satır başına en çok karakter:", x = 0, y = 0, width = 1, height = 1 },
      { class = "intedit", name = "cpl", value = 42, min = 10, max = 100, x = 1, y = 0, width = 1, height = 1 },
    }, { "Uygula", "İptal" }, { ok = "Uygula", cancel = "İptal" })
    if b ~= "Uygula" then aegisub.cancel() end
    return each_selected(subs, sel, function(t) return kaede.balance_lines(t, r.cpl) end)
  end)
