-- utf8.lua: Aegisub-style UTF-8 helpers (pure Lua, LuaJIT friendly)
local utf8 = {}

function utf8.charrange(s, i)
  local c = s:byte(i)
  if not c then return nil end
  if c < 0x80 then return 1
  elseif c >= 0xF0 then return 4
  elseif c >= 0xE0 then return 3
  elseif c >= 0xC0 then return 2 end
  return 1
end

function utf8.chars(s)
  local i = 1
  return function()
    if i > #s then return nil end
    local n = utf8.charrange(s, i)
    local c = s:sub(i, i + n - 1)
    i = i + n
    return c
  end
end

function utf8.len(s)
  local n, i = 0, 1
  while i <= #s do i = i + utf8.charrange(s, i); n = n + 1 end
  return n
end

function utf8.codepoint(s, i)
  i = i or 1
  local c = s:byte(i)
  if not c then return nil end
  local n = utf8.charrange(s, i)
  if n == 1 then return c end
  local cp = c % (2 ^ (7 - n))
  for k = 1, n - 1 do cp = cp * 64 + ((s:byte(i + k) or 0) % 64) end
  return math.floor(cp)
end
utf8.charcodepoint = utf8.codepoint

function utf8.char(...)
  local out = {}
  for _, cp in ipairs({...}) do
    if cp < 0x80 then out[#out + 1] = string.char(cp)
    elseif cp < 0x800 then out[#out + 1] = string.char(0xC0 + math.floor(cp / 64), 0x80 + cp % 64)
    elseif cp < 0x10000 then out[#out + 1] = string.char(0xE0 + math.floor(cp / 4096), 0x80 + math.floor(cp / 64) % 64, 0x80 + cp % 64)
    else out[#out + 1] = string.char(0xF0 + math.floor(cp / 262144), 0x80 + math.floor(cp / 4096) % 64, 0x80 + math.floor(cp / 64) % 64, 0x80 + cp % 64) end
  end
  return table.concat(out)
end
utf8.encode = utf8.char

function utf8.sub(s, i, j)
  local len = utf8.len(s)
  i = i or 1; j = j or -1
  if i < 0 then i = len + i + 1 end
  if j < 0 then j = len + j + 1 end
  if i < 1 then i = 1 end
  if j > len then j = len end
  if i > j then return "" end
  local pos, n, bs, be = 1, 0, nil, nil
  while pos <= #s do
    n = n + 1
    local w = utf8.charrange(s, pos)
    if n == i then bs = pos end
    if n == j then be = pos + w - 1; break end
    pos = pos + w
  end
  return s:sub(bs, be)
end

function utf8.reverse(s)
  local t = {}
  for c in utf8.chars(s) do table.insert(t, 1, c) end
  return table.concat(t)
end

return utf8
