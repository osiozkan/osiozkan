-- unicode.lua: UTF-8 helpers mirroring Aegisub's include/unicode.lua API.
local unicode = {}

function unicode.charwidth(s)
  local b = s:byte(1)
  if not b then return 0 end
  if b < 0x80 then return 1 elseif b < 0xE0 then return 2 elseif b < 0xF0 then return 3 else return 4 end
end

function unicode.codepoint(s)
  local b = s:byte(1)
  if not b then return nil end
  if b < 0x80 then return b end
  if b < 0xE0 then return (b - 0xC0) * 64 + (s:byte(2) - 0x80) end
  if b < 0xF0 then return (b - 0xE0) * 4096 + (s:byte(2) - 0x80) * 64 + (s:byte(3) - 0x80) end
  return (b - 0xF0) * 262144 + (s:byte(2) - 0x80) * 4096 + (s:byte(3) - 0x80) * 64 + (s:byte(4) - 0x80)
end

function unicode.chars(s)
  local pos = 1
  return function()
    if pos > #s then return nil end
    local w = unicode.charwidth(s:sub(pos, pos))
    local c = s:sub(pos, pos + w - 1)
    pos = pos + w
    return c
  end
end

function unicode.len(s)
  local n = 0
  for _ in unicode.chars(s) do n = n + 1 end
  return n
end

function unicode.to_char(cp)
  if cp < 0x80 then return string.char(cp) end
  if cp < 0x800 then return string.char(0xC0 + math.floor(cp / 64), 0x80 + cp % 64) end
  if cp < 0x10000 then return string.char(0xE0 + math.floor(cp / 4096), 0x80 + math.floor(cp / 64) % 64, 0x80 + cp % 64) end
  return string.char(0xF0 + math.floor(cp / 262144), 0x80 + math.floor(cp / 4096) % 64, 0x80 + math.floor(cp / 64) % 64, 0x80 + cp % 64)
end

function unicode.sub(s, i, j)
  local chars = {}
  for c in unicode.chars(s) do chars[#chars + 1] = c end
  local n = #chars
  i = i or 1
  j = j or n
  if i < 0 then i = n + i + 1 end
  if j < 0 then j = n + j + 1 end
  if i < 1 then i = 1 end
  if j > n then j = n end
  return table.concat(chars, "", i, j)
end

-- Case mapping: ASCII + Latin-1 supplement + Turkish letters (sufficient for fansub work).
local lower_map, upper_map = {}, {}
local function pair(l, u) lower_map[u] = l; upper_map[l] = u end
for c = 0xE0, 0xFE do if c ~= 0xF7 then pair(unicode.to_char(c), unicode.to_char(c - 32)) end end
pair("ş", "Ş"); pair("ğ", "Ğ"); pair("ı", "I"); pair("i", "İ")
lower_map["I"] = nil -- plain I lowercases to i unless a locale says otherwise
for c = 0x3B1, 0x3C9 do if c ~= 0x3C2 then pair(unicode.to_char(c), unicode.to_char(c - 32)) end end
for c = 0x430, 0x44F do pair(unicode.to_char(c), unicode.to_char(c - 32)) end

function unicode.lower(s)
  local out = {}
  for c in unicode.chars(s) do out[#out + 1] = lower_map[c] or c:lower() end
  return table.concat(out)
end
function unicode.upper(s)
  local out = {}
  for c in unicode.chars(s) do out[#out + 1] = (c == "i") and "I" or (upper_map[c] or c:upper()) end
  return table.concat(out)
end
unicode.to_lower_case, unicode.to_upper_case = unicode.lower, unicode.upper
function unicode.to_title_case(s)
  return (s:gsub("(%S)(%S*)", function(a, b) return unicode.upper(a) .. unicode.lower(b) end))
end

-- Turkish-aware variants (dotted/dotless i)
function unicode.lower_tr(s)
  return unicode.lower((s:gsub("I", "ı"):gsub("İ", "i")))
end
function unicode.upper_tr(s)
  return unicode.upper((s:gsub("i", "İ"):gsub("ı", "I")))
end

-- Aegisub also exposes these on the string table for convenience in some scripts.
_G.unicode = unicode
return unicode
