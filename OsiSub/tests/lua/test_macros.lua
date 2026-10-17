-- OsiSub Automation uyumluluk testleri için örnek makrolar
script_name = "Test makroları"
script_description = "Uyumluluk testleri"
script_author = "OsiSub"
script_version = "1.0"

include("karaskel.lua")
local re = require "aegisub.re"
local util = require "aegisub.util"

local function upper(subs, sel)
  for _, i in ipairs(sel) do
    local l = subs[i]
    l.text = unicode.to_upper_case(l.text)
    subs[i] = l
  end
  aegisub.set_undo_point("Büyük harf")
  return sel
end

local function split_n(subs, sel)
  local newsel = {}
  for k = #sel, 1, -1 do
    local i = sel[k]
    local l = subs[i]
    local a, b = l.text:match("^(.-)\\N(.*)$")
    if a then
      local mid = math.floor((l.start_time + l.end_time) / 2)
      local l2 = util.copy(l)
      l.text, l.end_time = a, mid
      l2.text, l2.start_time = b, mid
      subs[i] = l
      subs.insert(i + 1, l2)
    end
  end
  return newsel
end

local function del_comments(subs)
  local del = {}
  for i = 1, #subs do
    if subs[i].class == "dialogue" and subs[i].comment then del[#del + 1] = i end
  end
  subs.delete(del)
end

local function re_test(subs, sel)
  for _, i in ipairs(sel) do
    local l = subs[i]
    l.text = re.sub(l.text, "(\\w+)ya", "[$1]ya")
    l.effect = tostring(#re.split("a,b,,c", ","))
    subs[i] = l
  end
end

local function kara(subs, sel)
  local meta, styles = karaskel.collect_head(subs, false)
  for _, i in ipairs(sel) do
    local l = subs[i]
    karaskel.preproc_line(subs, meta, styles, l)
    local parts = {}
    for s = 1, l.kara.n do
      local syl = l.kara[s]
      parts[#parts + 1] = string.format("%s:%d:%d", syl.text_stripped, syl.start_time, math.floor(syl.width + 0.5))
    end
    l.effect = table.concat(parts, "|")
    subs[i] = l
  end
end

local function cancel_me(subs, sel)
  local l = subs[sel[1]]
  l.text = "BOZULDU"
  subs[sel[1]] = l
  aegisub.cancel()
end

local function fail(subs, sel)
  local x = nil
  return x.y
end

local function dlg(subs, sel)
  local btn, res = aegisub.dialog.display({
    {class = "label", label = "Metin:", x = 0, y = 0},
    {class = "edit", name = "txt", text = "varsayılan", x = 1, y = 0},
    {class = "checkbox", name = "up", label = "Büyüt", value = false, x = 0, y = 1},
    {class = "intedit", name = "n", value = 3, min = 0, max = 10, x = 1, y = 1},
  }, {"Tamam", "İptal"})
  if btn ~= "Tamam" then aegisub.cancel() end
  for _, i in ipairs(sel) do
    local l = subs[i]
    local t = res.txt
    if res.up then t = unicode.to_upper_case(t) end
    l.text = t .. "#" .. res.n
    subs[i] = l
  end
end

local function info_style(subs)
  for i = 1, #subs do
    local l = subs[i]
    if l.class == "info" and l.key == "Title" then l.value = "Yeni Başlık"; subs[i] = l end
    if l.class == "style" and l.name == "Sign" then l.fontsize = 66.5; l.bold = true; subs[i] = l end
  end
  subs.append({class = "info", section = "[Script Info]", key = "OsiTest", value = "1"})
  local s = subs[1]
  for i = 1, #subs do if subs[i].class == "style" then s = subs[i] break end end
  s.name = "Yeni"
  -- stil ekle: son stilin hemen arkasına
  local last = 0
  for i = 1, #subs do if subs[i].class == "style" then last = i end end
  subs.insert(last + 1, s)
end

local function readonly(subs, sel) local n = 0 for i = 1, #subs do n = n + #subs[i].raw end end

local function frames(subs, sel)
  local l = subs[sel[1]]
  l.effect = tostring(aegisub.frame_from_ms(l.start_time)) .. "," .. tostring(aegisub.ms_from_frame(24))
  subs[sel[1]] = l
end

aegisub.register_macro("Test/Büyük harf", "", upper)
aegisub.register_macro("Test/Böl", "", split_n)
aegisub.register_macro("Test/Yorumları sil", "", del_comments)
aegisub.register_macro("Test/re", "", re_test)
aegisub.register_macro("Test/karaskel", "", kara)
aegisub.register_macro("Test/iptal", "", cancel_me)
aegisub.register_macro("Test/hata", "", fail)
aegisub.register_macro("Test/dialog", "", dlg)
aegisub.register_macro("Test/info-style", "", info_style)
aegisub.register_macro("Test/salt-okuma", "", readonly)
aegisub.register_macro("Test/kare", "", frames, function(subs, sel) return #sel == 1, "tek satır" end)
