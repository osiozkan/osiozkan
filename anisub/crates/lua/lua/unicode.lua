-- unicode.lua: UTF-8 helpers compatible with Aegisub's include("unicode.lua").
unicode = unicode or {}

function unicode.charwidth(c)
  local b = c:byte(1)
  if not b then return 0 end
  if b < 0x80 then return 1 elseif b < 0xE0 then return 2 elseif b < 0xF0 then return 3 else return 4 end
end

function unicode.chars(s)
  return coroutine.wrap(function()
    for c in s:gmatch("[%z\1-\127\194-\244][\128-\191]*") do coroutine.yield(c) end
  end)
end

function unicode.len(s)
  local n = 0
  for _ in s:gmatch("[%z\1-\127\194-\244][\128-\191]*") do n = n + 1 end
  return n
end

function unicode.codepoint(c)
  local b = { c:byte(1, -1) }
  local n = #b
  if n == 0 then return nil end
  if n == 1 then return b[1] end
  local cp = b[1] % (n == 2 and 0x20 or n == 3 and 0x10 or 0x08)
  for i = 2, n do cp = cp * 64 + (b[i] % 64) end
  return cp
end

local tr_up = { ["i"] = "İ", ["ı"] = "I" }
local tr_lo = { ["I"] = "ı", ["İ"] = "i" }
function unicode.to_upper_case(s) return (s:upper():gsub("ı", "I")) end
function unicode.to_lower_case(s) return (s:lower():gsub("İ", "i")) end
function unicode.to_tr_upper(s)
  local o = {}
  for c in unicode.chars(s) do o[#o + 1] = tr_up[c] or c:upper() end
  return table.concat(o)
end
function unicode.to_tr_lower(s)
  local o = {}
  for c in unicode.chars(s) do o[#o + 1] = tr_lo[c] or c:lower() end
  return table.concat(o)
end
