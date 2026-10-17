-- aegisub.unicode — UTF-8 yardımcıları (Aegisub unicode.moon'un düz Lua karşılığı).
-- Özgün: Copyright (c) 2012, Thomas Goyne (ISC/BSD). Büyük/küçük harf
-- dönüşümleri Aegisub'da ICU ile yapılır; burada Python'un Unicode
-- tablolarıyla yapılır (kök yerel ayar: i -> I, Aegisub ile aynı).

local py = __osisub_py
local unicode

unicode = {
  charwidth = function(s, i)
    local b = s:byte(i or 1)
    if not b then return 1
    elseif b < 128 then return 1
    elseif b < 224 then return 2
    elseif b < 240 then return 3
    else return 4 end
  end,

  chars = function(s)
    if type(s) ~= "string" then error("unicode.chars: metin bekleniyordu", 2) end
    local curchar, i = 0, 1
    return function()
      if i > #s then return end
      local j = i
      curchar = curchar + 1
      i = i + unicode.charwidth(s, i)
      return s:sub(j, i - 1), curchar
    end
  end,

  len = function(s)
    if type(s) ~= "string" then error("unicode.len: metin bekleniyordu", 2) end
    local n = 0
    for _ in unicode.chars(s) do n = n + 1 end
    return n
  end,

  codepoint = function(s)
    local b = s:byte(1)
    if b < 128 then return b end
    local res, w
    if b < 224 then res, w = b - 192, 2
    elseif b < 240 then res, w = b - 224, 3
    else res, w = b - 240, 4 end
    for i = 2, w do res = res * 64 + s:byte(i) - 128 end
    return res
  end,

  to_upper_case = function(s) return py.str_upper(s) end,
  to_lower_case = function(s) return py.str_lower(s) end,
  to_fold_case = function(s) return py.str_fold(s) end,
}

return unicode
