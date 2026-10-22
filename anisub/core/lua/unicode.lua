-- AniSub unicode.lua : UTF-8 helpers (Aegisub compatible subset)
unicode = unicode or {}

function unicode.codepoint(ch)
  local b = ch:byte(1)
  if not b then return nil end
  if b < 0x80 then return b end
  local n, cp
  if b >= 0xF0 then n, cp = 3, b % 8 elseif b >= 0xE0 then n, cp = 2, b % 16 else n, cp = 1, b % 32 end
  for i = 2, n + 1 do cp = cp * 64 + ((ch:byte(i) or 0) % 64) end
  return cp
end

function unicode.chars(s)
  local i = 1
  return function()
    if i > #s then return nil end
    local b = s:byte(i)
    local n = b < 0x80 and 1 or b >= 0xF0 and 4 or b >= 0xE0 and 3 or b >= 0xC0 and 2 or 1
    local c = s:sub(i, i + n - 1)
    i = i + n
    return c
  end
end

function unicode.len(s)
  local _, n = s:gsub("[^\128-\191]", "")
  return n
end

function unicode.charwidth(ch)
  return host_charwidth and host_charwidth(ch) or 1
end
