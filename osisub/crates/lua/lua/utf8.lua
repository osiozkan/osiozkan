-- Minimal "utf8" module compatible with the commonly used subset of Aegisub's utf8.lua
local utf8 = {}
local pat = "[%z\1-\127\194-\244][\128-\191]*"

function utf8.len(s) local _, n = s:gsub("[^\128-\191]", ""); return n end
function utf8.chars(s) return s:gmatch(pat) end
function utf8.char(...)
  local out = {}
  for i, cp in ipairs({ ... }) do
    if cp < 0x80 then out[i] = string.char(cp)
    elseif cp < 0x800 then out[i] = string.char(0xC0 + math.floor(cp / 64), 0x80 + cp % 64)
    elseif cp < 0x10000 then out[i] = string.char(0xE0 + math.floor(cp / 4096), 0x80 + math.floor(cp / 64) % 64, 0x80 + cp % 64)
    else out[i] = string.char(0xF0 + math.floor(cp / 262144), 0x80 + math.floor(cp / 4096) % 64, 0x80 + math.floor(cp / 64) % 64, 0x80 + cp % 64) end
  end
  return table.concat(out)
end
function utf8.sub(s, i, j)
  local t = {}
  for c in s:gmatch(pat) do t[#t + 1] = c end
  local n = #t
  i = i or 1; j = j or n
  if i < 0 then i = math.max(n + i + 1, 1) elseif i == 0 then i = 1 end
  if j < 0 then j = n + j + 1 elseif j > n then j = n end
  if i > j then return "" end
  return table.concat(t, "", i, j)
end
function utf8.reverse(s)
  local t = {}
  for c in s:gmatch(pat) do table.insert(t, 1, c) end
  return table.concat(t)
end
function utf8.upper(s) return __osisub_upper(s) end
function utf8.lower(s) return __osisub_lower(s) end
function utf8.codepoint(ch) return require("unicode").codepoint(ch) end

return utf8
