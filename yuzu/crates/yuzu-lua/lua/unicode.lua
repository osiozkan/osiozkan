-- unicode.lua (Aegisub-compatible): UTF-8 helpers
local unicode = {}

function unicode.chars(s)
  local it = s:gmatch("[%z\1-\127\194-\244][\128-\191]*")
  return it
end

function unicode.len(s)
  local n = 0
  for _ in s:gmatch("[%z\1-\127\194-\244][\128-\191]*") do n = n + 1 end
  return n
end

function unicode.codepoint(c)
  local b = c:byte(1)
  if not b then return nil end
  if b < 0x80 then return b end
  local n, cp
  if b >= 0xF0 then n, cp = 3, b - 0xF0
  elseif b >= 0xE0 then n, cp = 2, b - 0xE0
  else n, cp = 1, b - 0xC0 end
  for i = 2, n + 1 do cp = cp * 64 + ((c:byte(i) or 128) - 128) end
  return cp
end

function unicode.char(cp)
  if cp < 0x80 then return string.char(cp) end
  if cp < 0x800 then return string.char(0xC0 + math.floor(cp / 64), 0x80 + cp % 64) end
  if cp < 0x10000 then return string.char(0xE0 + math.floor(cp / 4096), 0x80 + math.floor(cp / 64) % 64, 0x80 + cp % 64) end
  return string.char(0xF0 + math.floor(cp / 262144), 0x80 + math.floor(cp / 4096) % 64, 0x80 + math.floor(cp / 64) % 64, 0x80 + cp % 64)
end

function unicode.sub(s, i, j)
  local t = {}
  for c in unicode.chars(s) do t[#t + 1] = c end
  local n = #t
  if i < 0 then i = n + i + 1 end
  j = j or n
  if j < 0 then j = n + j + 1 end
  return table.concat(t, "", math.max(1, i), math.min(n, j))
end

function unicode.reverse(s)
  local t = {}
  for c in unicode.chars(s) do table.insert(t, 1, c) end
  return table.concat(t)
end

local trmap_up = { ["ı"] = "I", ["i"] = "İ" }
local trmap_lo = { ["I"] = "ı", ["İ"] = "i" }
function unicode.to_upper_case(s) return __yuzu_upper(s) end
function unicode.to_lower_case(s) return __yuzu_lower(s) end
function unicode.to_fold_case(s) return __yuzu_lower(s) end
unicode.upper = unicode.to_upper_case
unicode.lower = unicode.to_lower_case

function unicode.is_cjk(cp)
  return (cp >= 0x3040 and cp <= 0x30FF) or (cp >= 0x4E00 and cp <= 0x9FFF) or (cp >= 0xFF00 and cp <= 0xFFEF)
end

return unicode
