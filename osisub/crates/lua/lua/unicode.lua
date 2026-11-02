-- Aegisub "unicode" module (subset) on top of UTF-8 strings
local unicode = {}

function unicode.codepoint(ch)
  local b = ch:byte(1)
  if not b then return nil end
  if b < 0x80 then return b end
  local n = b >= 0xF0 and 3 or (b >= 0xE0 and 2 or 1)
  local cp = b % (2 ^ (6 - n))
  for i = 2, n + 1 do cp = cp * 64 + (ch:byte(i) or 0) % 64 end
  return cp
end

function unicode.chars(s)
  return s:gmatch("[%z\1-\127\194-\244][\128-\191]*")
end

function unicode.len(s)
  local _, n = s:gsub("[^\128-\191]", "")
  return n
end

function unicode.to_upper_case(s) return __osisub_upper(s) end
function unicode.to_lower_case(s) return __osisub_lower(s) end
function unicode.to_fold_case(s) return __osisub_lower(s) end
function unicode.to_title_case(s)
  return (s:gsub("(%S)(%S*)", function(a, b) return __osisub_upper(a) .. __osisub_lower(b) end))
end

return unicode
