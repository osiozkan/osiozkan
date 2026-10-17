-- aegisub.parse_karaoke_data(line) — Aegisub'ın C++ AssKaraoke ayrıştırıcısının
-- Lua karşılığı. karaskel bu işleve dayanır.
--
-- Dönüş: 0'dan başlayan dizi. kara[0] her zaman boş bir "hece"dir (2.1.x
-- uyumluluğu için Aegisub da öyle yapar). Her hece:
--   duration, start_time, end_time (satır başına göre ms), tag ("\k", "\kf", "\ko"),
--   text (etiketler dahil, \k hariç), text_stripped (düz metin)

local function new_syl(start_time)
  return {start_time = start_time, duration = 0, tag = "\\k", full = {}, plain = {}}
end

local function finish(syl, line_start)
  local s = table.concat(syl.plain)
  return {
    duration = syl.duration,
    start_time = syl.start_time - line_start,
    end_time = syl.start_time + syl.duration - line_start,
    tag = syl.tag,
    text = table.concat(syl.full),
    text_stripped = s,
  }
end

local function split_tags(block)
  -- "\b1\k20\i1" -> {"\b1", "\k20", "\i1"}; parantez içindeki \ (\t, \clip) bölünmez
  local tags = {}
  local depth, cur = 0, nil
  for i = 1, #block do
    local c = block:sub(i, i)
    if c == "\\" and depth == 0 then
      if cur then tags[#tags + 1] = cur end
      cur = "\\"
    else
      if c == "(" then depth = depth + 1 elseif c == ")" and depth > 0 then depth = depth - 1 end
      if cur then cur = cur .. c end
    end
  end
  if cur then tags[#tags + 1] = cur end
  return tags
end

local function kara_tag(tag)
  local name, num = tag:match("^\\(k[fo]?)%s*([%d%.%-]*)")
  if not name then
    name, num = tag:match("^\\(K)%s*([%d%.%-]*)")
  end
  if not name then return nil end
  if name == "K" then name = "kf" end
  return "\\" .. name, math.floor(tonumber(num) or 0)
end

function aegisub.parse_karaoke_data(line)
  if type(line) ~= "table" or line.class ~= "dialogue" then
    error("Subtitle line must be a dialogue line", 2)
  end
  local text = line.text or ""
  local line_start = line.start_time or 0
  local result = {[0] = {duration = 0, start_time = 0, end_time = 0, tag = "",
                         text = "", text_stripped = ""}}
  local syls = {}
  local syl = new_syl(line_start)
  local drawing = false
  local pos = 1
  local n = #text
  while pos <= n do
    local bstart = text:find("{", pos, true)
    local bend = bstart and text:find("}", bstart + 1, true)
    local plain_end = (bstart and bend) and (bstart - 1) or n
    if plain_end >= pos then
      local plain = text:sub(pos, plain_end)
      if drawing then
        syl.full[#syl.full + 1] = plain
      else
        syl.full[#syl.full + 1] = plain
        syl.plain[#syl.plain + 1] = plain
      end
    end
    if not (bstart and bend) then break end
    local inner = text:sub(bstart + 1, bend - 1)
    if not inner:find("\\", 1, true) then
      -- yorum bloğu
      syl.full[#syl.full + 1] = "{" .. inner .. "}"
    else
      local in_tag = false
      for _, tag in ipairs(split_tags(inner)) do
        local kname, kdur = kara_tag(tag)
        if kname then
          if in_tag then
            syl.full[#syl.full + 1] = "}"
            in_tag = false
          end
          if syl.duration > 0 or #syl.plain > 0 and table.concat(syl.plain) ~= "" then
            syls[#syls + 1] = syl
            syl = {start_time = syl.start_time, duration = syl.duration, tag = syl.tag, full = {}, plain = {}}
          end
          syl.tag = kname
          syl.start_time = syl.start_time + syl.duration
          syl.duration = kdur * 10
        else
          local p = tag:match("^\\p%s*(%d+)")
          if p then drawing = tonumber(p) > 0 end
          if not in_tag then syl.full[#syl.full + 1] = "{" end
          in_tag = true
          syl.full[#syl.full + 1] = tag
        end
      end
      if in_tag then syl.full[#syl.full + 1] = "}" end
    end
    pos = bend + 1
  end
  syls[#syls + 1] = syl
  for i, s in ipairs(syls) do result[i] = finish(s, line_start) end
  return result
end
