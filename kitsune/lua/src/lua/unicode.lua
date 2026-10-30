-- UTF-8 yardımcıları (Aegisub unicode.lua/utf8 uyumlu alt küme).
local unicode = {}

local function charlen(b)
  if b < 0x80 then return 1 elseif b >= 0xF0 then return 4 elseif b >= 0xE0 then return 3 elseif b >= 0xC0 then return 2 end
  return 1
end

function unicode.charwidth(s, i)
  return charlen(s:byte(i or 1))
end

function unicode.chars(s)
  local i = 1
  return function()
    if i > #s then return nil end
    local n = charlen(s:byte(i))
    local c = s:sub(i, i + n - 1)
    i = i + n
    return c
  end
end

function unicode.len(s)
  local n = 0
  for _ in unicode.chars(s) do n = n + 1 end
  return n
end

function unicode.codepoint(c)
  local b = c:byte(1)
  if not b then return nil end
  if b < 0x80 then return b end
  if b >= 0xF0 then return ((b % 8) * 262144) + ((c:byte(2) % 64) * 4096) + ((c:byte(3) % 64) * 64) + (c:byte(4) % 64) end
  if b >= 0xE0 then return ((b % 16) * 4096) + ((c:byte(2) % 64) * 64) + (c:byte(3) % 64) end
  return ((b % 32) * 64) + (c:byte(2) % 64)
end

function unicode.char(cp)
  if cp < 0x80 then return string.char(cp) end
  if cp < 0x800 then return string.char(0xC0 + math.floor(cp / 64), 0x80 + cp % 64) end
  if cp < 0x10000 then return string.char(0xE0 + math.floor(cp / 4096), 0x80 + math.floor(cp / 64) % 64, 0x80 + cp % 64) end
  return string.char(0xF0 + math.floor(cp / 262144), 0x80 + math.floor(cp / 4096) % 64, 0x80 + math.floor(cp / 64) % 64, 0x80 + cp % 64)
end

-- Aegisub'in "utf8" modülü ile aynı işlevler
unicode.charcodepoint = unicode.codepoint
return unicode
