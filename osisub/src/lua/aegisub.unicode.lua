-- aegisub.unicode: native module of Aegisub, reimplemented on Lua 5.3 utf8
local unicode = {}
-- number of bytes of the UTF-8 character starting at byte position i
function unicode.charwidth(s, i)
  i = i or 1
  local b = s:byte(i); if not b then return 0 end
  if b < 0x80 then return 1 elseif b >= 0xF0 then return 4 elseif b >= 0xE0 then return 3 elseif b >= 0xC0 then return 2 end
  return 1
end
function unicode.chars(s)
  local i = 1
  return function()
    if i > #s then return nil end
    local w = unicode.charwidth(s, i); local c = s:sub(i, i + w - 1); i = i + w; return c
  end
end
function unicode.len(s) return utf8.len(s) or #s end
function unicode.codepoint(s) return utf8.codepoint(s, 1) end
function unicode.to_upper(s) return (__host('case', __json.encode({ 'upper', s }))) end
function unicode.to_lower(s) return (__host('case', __json.encode({ 'lower', s }))) end
function unicode.to_fold(s) return unicode.to_lower(s) end
unicode.upper, unicode.lower = unicode.to_upper, unicode.to_lower
return unicode
