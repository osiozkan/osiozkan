-- osisub: utf8.lua (Aegisub'ın paketlediği sürümle uyumlu temel işlevler)
local uni = aegisub.__unicode
local utf8 = {}

function utf8.charbytes(s, i)
  i = i or 1
  local c = s:byte(i)
  if not c then return 0 end
  if c <= 127 then return 1
  elseif c >= 194 and c <= 223 then return 2
  elseif c >= 224 and c <= 239 then return 3
  elseif c >= 240 and c <= 244 then return 4 end
  return 1
end

function utf8.len(s)
  local pos, n = 1, 0
  while pos <= #s do
    pos = pos + utf8.charbytes(s, pos)
    n = n + 1
  end
  return n
end

-- karakter dizinine (1 tabanlı, negatif sondan) karşılık gelen bayt konumu
function utf8.charpos(s, ci)
  local len = utf8.len(s)
  if ci < 0 then ci = len + ci + 1 end
  local pos = 1
  for _ = 1, ci - 1 do
    if pos > #s then break end
    pos = pos + utf8.charbytes(s, pos)
  end
  return pos
end

function utf8.sub(s, i, j)
  j = j or -1
  local len = utf8.len(s)
  if i < 0 then i = len + i + 1 end
  if j < 0 then j = len + j + 1 end
  if i < 1 then i = 1 end
  if j > len then j = len end
  if i > j then return "" end
  local a = utf8.charpos(s, i)
  local b = utf8.charpos(s, j + 1) - 1
  return s:sub(a, b)
end

function utf8.chars(s)
  local pos = 1
  return function()
    if pos > #s then return nil end
    local n = utf8.charbytes(s, pos)
    local c = s:sub(pos, pos + n - 1)
    pos = pos + n
    return c
  end
end

function utf8.reverse(s)
  local t = {}
  for c in utf8.chars(s) do table.insert(t, 1, c) end
  return table.concat(t)
end

function utf8.codepoint(s, i)
  i = i or 1
  local n = utf8.charbytes(s, i)
  local b = { s:byte(i, i + n - 1) }
  if n == 1 then return b[1] end
  local cp = b[1] % (2 ^ (7 - n))
  for k = 2, n do cp = cp * 64 + (b[k] % 64) end
  return cp
end

function utf8.char(...)
  local out = {}
  for _, cp in ipairs({ ... }) do
    if cp < 0x80 then out[#out + 1] = string.char(cp)
    elseif cp < 0x800 then out[#out + 1] = string.char(0xC0 + math.floor(cp / 64), 0x80 + cp % 64)
    elseif cp < 0x10000 then out[#out + 1] = string.char(0xE0 + math.floor(cp / 4096), 0x80 + math.floor(cp / 64) % 64, 0x80 + cp % 64)
    else out[#out + 1] = string.char(0xF0 + math.floor(cp / 262144), 0x80 + math.floor(cp / 4096) % 64, 0x80 + math.floor(cp / 64) % 64, 0x80 + cp % 64) end
  end
  return table.concat(out)
end

function utf8.upper(s) return uni.to_upper(s) end
function utf8.lower(s) return uni.to_lower(s) end

return utf8
