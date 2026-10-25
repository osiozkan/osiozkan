-- Aegisub `unicode` module on top of Lua 5.3 utf8
local unicode = {}
function unicode.len(s) return utf8.len(s) or #s end
function unicode.chars(s) return utf8.codes and (function() local i = 0; return function() local st = utf8.offset(s, i + 1); if not st or st > #s then return nil end; local en = (utf8.offset(s, i + 2) or #s + 1) - 1; i = i + 1; return s:sub(st, en) end end)() end
function unicode.codepoint(s) return utf8.codepoint(s, 1) end
function unicode.charwidth(s) return 1 end
function unicode.sub(s, i, j)
  local n = utf8.len(s) or #s; i = i or 1; j = j or n
  if i < 0 then i = n + i + 1 end; if j < 0 then j = n + j + 1 end
  if i < 1 then i = 1 end; if j > n then j = n end
  if i > j then return '' end
  return s:sub(utf8.offset(s, i), (utf8.offset(s, j + 1) or #s + 1) - 1)
end
function unicode.reverse(s) local t = {}; for _, c in utf8.codes(s) do table.insert(t, 1, utf8.char(c)) end; return table.concat(t) end
unicode.upper = function(s) return host_case and host_case('upper', s) or s:upper() end
unicode.lower = function(s) return host_case and host_case('lower', s) or s:lower() end
return unicode
