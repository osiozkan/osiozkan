-- Minimal utf8 library (Lua 5.3 API subset) for Lua 5.1
local utf8 = {}
utf8.charpattern = "[\0-\127\194-\244][\128-\191]*"
function utf8.char(...)
  local t = {}
  for i, c in ipairs({ ... }) do
    if c < 0x80 then t[i] = string.char(c)
    elseif c < 0x800 then t[i] = string.char(0xC0 + math.floor(c / 64), 0x80 + c % 64)
    elseif c < 0x10000 then t[i] = string.char(0xE0 + math.floor(c / 4096), 0x80 + math.floor(c / 64) % 64, 0x80 + c % 64)
    else t[i] = string.char(0xF0 + math.floor(c / 262144), 0x80 + math.floor(c / 4096) % 64, 0x80 + math.floor(c / 64) % 64, 0x80 + c % 64) end
  end
  return table.concat(t)
end
function utf8.len(s, i, j)
  i = i or 1; j = j or #s
  local n = 0
  for p = i, j do local b = s:byte(p); if b < 0x80 or b >= 0xC0 then n = n + 1 end end
  return n
end
function utf8.codepoint(s, i, j)
  i = i or 1; j = j or i
  local out = {}
  local p = i
  while p <= j do
    local b = s:byte(p)
    local c, n
    if b < 0x80 then c, n = b, 1
    elseif b < 0xE0 then c, n = (b - 0xC0) * 64 + (s:byte(p + 1) - 0x80), 2
    elseif b < 0xF0 then c, n = (b - 0xE0) * 4096 + (s:byte(p + 1) - 0x80) * 64 + (s:byte(p + 2) - 0x80), 3
    else c, n = (b - 0xF0) * 262144 + (s:byte(p + 1) - 0x80) * 4096 + (s:byte(p + 2) - 0x80) * 64 + (s:byte(p + 3) - 0x80), 4 end
    out[#out + 1] = c; p = p + n
  end
  return unpack(out)
end
function utf8.codes(s)
  local p = 1
  return function()
    if p > #s then return nil end
    local c = utf8.codepoint(s, p)
    local b = s:byte(p)
    local n = b < 0x80 and 1 or b < 0xE0 and 2 or b < 0xF0 and 3 or 4
    local pos = p
    p = p + n
    return pos, c
  end
end
function utf8.offset(s, n, i)
  i = i or (n > 0 and 1 or #s + 1)
  if n > 0 then
    local p = i
    while n > 1 and p <= #s do p = p + 1; while p <= #s and s:byte(p) >= 0x80 and s:byte(p) < 0xC0 do p = p + 1 end; n = n - 1 end
    return p
  end
  return i
end
return utf8
