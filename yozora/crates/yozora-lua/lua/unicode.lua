-- Yozora unicode.lua: UTF-8 yardımcıları.
unicode = unicode or {}
local native = __yozora_native

function unicode.codepoint(c)
  return native.codepoint(c)
end

-- UTF-8 karakter yineleyici: (indeks, karakter)
function unicode.chars(s)
  local i, n = 0, 0
  local len = #s
  return function()
    if i >= len then return nil end
    local c = s:byte(i + 1)
    local l = 1
    if c >= 0xF0 then l = 4 elseif c >= 0xE0 then l = 3 elseif c >= 0xC0 then l = 2 end
    local ch = s:sub(i + 1, i + l)
    i = i + l
    n = n + 1
    return ch
  end
end

function unicode.len(s)
  local n = 0
  for _ in unicode.chars(s) do n = n + 1 end
  return n
end

function unicode.charwidth(c) return native.charwidth(c) end

function unicode.to_upper_case(s) return native.upper(s) end
function unicode.to_lower_case(s) return native.lower(s) end
function unicode.to_fold_case(s) return native.fold(s) end
unicode.to_upper = unicode.to_upper_case
unicode.to_lower = unicode.to_lower_case

function unicode.sub(s, i, j)
  local chars = {}
  for c in unicode.chars(s) do chars[#chars + 1] = c end
  local n = #chars
  j = j or n
  if i < 0 then i = n + i + 1 end
  if j < 0 then j = n + j + 1 end
  if i < 1 then i = 1 end
  if j > n then j = n end
  return table.concat(chars, "", i, j)
end

return unicode
