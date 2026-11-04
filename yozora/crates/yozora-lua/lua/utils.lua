-- Yozora utils.lua: Aegisub `utils.lua` ile uyumlu yardımcı işlevler.

util = util or {}

function string.trim(s)
  return (s:gsub("^%s+", ""):gsub("%s+$", ""))
end

-- İlk sözcük ve geri kalan.
function string.headtail(s)
  local a, b, head, tail = s:find("(.-)%s+(.*)")
  if a then return head, tail end
  return s, ""
end

function string.words(s)
  return s:gmatch("%S+")
end

function table.copy(t)
  local r = {}
  for k, v in pairs(t) do r[k] = v end
  return r
end

function table.copy_deep(t, seen)
  if type(t) ~= "table" then return t end
  seen = seen or {}
  if seen[t] then return seen[t] end
  local r = {}
  seen[t] = r
  for k, v in pairs(t) do r[table.copy_deep(k, seen)] = table.copy_deep(v, seen) end
  return setmetatable(r, getmetatable(t))
end

function table.append(a, b)
  for _, v in ipairs(b) do a[#a + 1] = v end
  return a
end

function table.indexof(t, v)
  for i, x in ipairs(t) do if x == v then return i end end
  return nil
end

function math.round(x, p)
  p = p or 0
  local m = 10 ^ p
  return math.floor(x * m + 0.5) / m
end

function clamp(v, lo, hi)
  if v < lo then return lo elseif v > hi then return hi end
  return v
end

-- Renk ayrıştırma: r, g, b, a döndürür (a ASS anlamında, 0 = opak).
function extract_color(s)
  local a, b, g, r
  -- &HAABBGGRR
  a, b, g, r = s:match("&[Hh](%x%x)(%x%x)(%x%x)(%x%x)")
  if a then return tonumber(r, 16), tonumber(g, 16), tonumber(b, 16), tonumber(a, 16) end
  -- &HBBGGRR&
  b, g, r = s:match("&[Hh](%x%x)(%x%x)(%x%x)&?")
  if b then return tonumber(r, 16), tonumber(g, 16), tonumber(b, 16), 0 end
  -- #RRGGBB
  r, g, b = s:match("#(%x%x)(%x%x)(%x%x)")
  if r then return tonumber(r, 16), tonumber(g, 16), tonumber(b, 16), 0 end
  -- &HAA&  (yalnızca alfa)
  a = s:match("&[Hh](%x%x)&")
  if a then return 0, 0, 0, tonumber(a, 16) end
  -- Ondalık sayı
  local n = tonumber(s)
  if n then
    n = n % 4294967296
    return n % 256, math.floor(n / 256) % 256, math.floor(n / 65536) % 256, math.floor(n / 16777216) % 256
  end
  error("extract_color: geçersiz renk: " .. tostring(s), 2)
end

function alpha_from_style(s)
  local _, _, _, a = extract_color(s)
  return string.format("&H%02X&", a)
end

function color_from_style(s)
  local r, g, b = extract_color(s)
  return string.format("&H%02X%02X%02X&", b, g, r)
end

function ass_alpha(a) return string.format("&H%02X&", clamp(math.floor(a + 0.5), 0, 255)) end
function ass_color(r, g, b)
  return string.format("&H%02X%02X%02X&", clamp(math.floor(b + 0.5), 0, 255), clamp(math.floor(g + 0.5), 0, 255), clamp(math.floor(r + 0.5), 0, 255))
end
function ass_style_color(r, g, b, a)
  return string.format("&H%02X%02X%02X%02X", clamp(math.floor((a or 0) + 0.5), 0, 255), clamp(math.floor(b + 0.5), 0, 255), clamp(math.floor(g + 0.5), 0, 255), clamp(math.floor(r + 0.5), 0, 255))
end

function interpolate(pct, a, b)
  if pct <= 0 then return a elseif pct >= 1 then return b end
  return pct * b + (1 - pct) * a
end

function interpolate_color(pct, c1, c2)
  local r1, g1, b1 = extract_color(c1)
  local r2, g2, b2 = extract_color(c2)
  return ass_color(interpolate(pct, r1, r2), interpolate(pct, g1, g2), interpolate(pct, b1, b2))
end

function interpolate_alpha(pct, a1, a2)
  local _, _, _, x = extract_color(a1)
  local _, _, _, y = extract_color(a2)
  return ass_alpha(interpolate(pct, x, y))
end

function HSV_to_RGB(H, S, V)
  H = H % 360
  S = clamp(S, 0, 1) V = clamp(V, 0, 1)
  local C = V * S
  local Hp = H / 60
  local X = C * (1 - math.abs(Hp % 2 - 1))
  local r, g, b = 0, 0, 0
  if Hp < 1 then r, g, b = C, X, 0
  elseif Hp < 2 then r, g, b = X, C, 0
  elseif Hp < 3 then r, g, b = 0, C, X
  elseif Hp < 4 then r, g, b = 0, X, C
  elseif Hp < 5 then r, g, b = X, 0, C
  else r, g, b = C, 0, X end
  local m = V - C
  return math.floor((r + m) * 255 + 0.5), math.floor((g + m) * 255 + 0.5), math.floor((b + m) * 255 + 0.5)
end

function RGB_to_HSV(r, g, b)
  r, g, b = r / 255, g / 255, b / 255
  local mx, mn = math.max(r, g, b), math.min(r, g, b)
  local d = mx - mn
  local H
  if d == 0 then H = 0
  elseif mx == r then H = 60 * (((g - b) / d) % 6)
  elseif mx == g then H = 60 * ((b - r) / d + 2)
  else H = 60 * ((r - g) / d + 4) end
  local S = mx == 0 and 0 or d / mx
  return H, S, mx
end

function HSL_to_RGB(H, S, L)
  H = H % 360
  S = clamp(S, 0, 1) L = clamp(L, 0, 1)
  local C = (1 - math.abs(2 * L - 1)) * S
  local Hp = H / 60
  local X = C * (1 - math.abs(Hp % 2 - 1))
  local r, g, b = 0, 0, 0
  if Hp < 1 then r, g, b = C, X, 0
  elseif Hp < 2 then r, g, b = X, C, 0
  elseif Hp < 3 then r, g, b = 0, C, X
  elseif Hp < 4 then r, g, b = 0, X, C
  elseif Hp < 5 then r, g, b = X, 0, C
  else r, g, b = C, 0, X end
  local m = L - C / 2
  return math.floor((r + m) * 255 + 0.5), math.floor((g + m) * 255 + 0.5), math.floor((b + m) * 255 + 0.5)
end

function RGB_to_HSL(r, g, b)
  r, g, b = r / 255, g / 255, b / 255
  local mx, mn = math.max(r, g, b), math.min(r, g, b)
  local d = mx - mn
  local L = (mx + mn) / 2
  local H, S = 0, 0
  if d ~= 0 then
    S = d / (1 - math.abs(2 * L - 1))
    if mx == r then H = 60 * (((g - b) / d) % 6)
    elseif mx == g then H = 60 * ((b - r) / d + 2)
    else H = 60 * ((r - g) / d + 4) end
  end
  return H, S, L
end

-- `util` ad alanı (aegisub.util / require "aegisub.util")
util.clamp = clamp
util.copy = table.copy
util.deep_copy = table.copy_deep
util.trim = string.trim
util.headtail = string.headtail
util.words = string.words
util.extract_color = extract_color
util.alpha_from_style = alpha_from_style
util.color_from_style = color_from_style
util.ass_alpha = ass_alpha
util.ass_color = ass_color
util.ass_style_color = ass_style_color
util.interpolate = interpolate
util.interpolate_color = interpolate_color
util.interpolate_alpha = interpolate_alpha
util.HSV_to_RGB = HSV_to_RGB
util.RGB_to_HSV = RGB_to_HSV
util.HSL_to_RGB = HSL_to_RGB
util.RGB_to_HSL = RGB_to_HSL
util.fix_alpha = function(s) return s end

return util
