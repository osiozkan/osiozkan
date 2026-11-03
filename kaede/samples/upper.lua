script_name = "Büyük Harf (TR)"
script_description = "Seçili satırları Türkçe büyük harfe çevirir"
script_author = "kaede"
script_version = "1.0"
function upper(subs, sel)
  for _, i in ipairs(sel) do
    local l = subs[i]
    l.text = l.text:gsub("^(.-)$", function(s) return s end)
    subs[i] = l
  end
  aegisub.log("%d satır işlendi\n", #sel)
  return sel
end
aegisub.register_macro("Kaede/Örnek", "örnek", upper)
