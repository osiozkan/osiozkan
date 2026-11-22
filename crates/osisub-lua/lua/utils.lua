-- osisub: Aegisub utils.lua uyumlu yardımcılar

util = {}

function util.copy(t)
  local r = {}
  for k, v in pairs(t) do r[k] = v end
  return r
end

function util.deep_copy(t, seen)
  if type(t) ~= "table" then return t end
  seen = seen or {}
  if seen[t] then return seen[t] end
  local r = {}
  seen[t] = r
  for k, v in pairs(t) do r[util.deep_copy(k, seen)] = util.deep_copy(v, seen) end
  return r -- Aegisub gibi metatablo kopyalanmaz
end

function util.clamp(v, lo, hi)
  if v < lo then return lo elseif v > hi then return hi end
  return v
end

local function round(x) return math.floor(x + 0.5) end
if not math.round then math.round = round end

-- Aegisub `%02X` ile biçimler: ondalık değer KESİLİR (127.5 → 7F), yuvarlanmaz. Aralık dışı
-- değerler Aegisub'da bozuk etiket üretirdi; burada sınırlanır.
local function byte(x) return util.clamp(math.floor(x), 0, 255) end

function util.ass_color(r, g, b)
  return string.format("&H%02X%02X%02X&", byte(b), byte(g), byte(r))
end

function util.ass_alpha(a)
  return string.format("&H%02X&", byte(a))
end

function util.ass_style_color(r, g, b, a)
  return string.format("&H%02X%02X%02X%02X", byte(a or 0), byte(b), byte(g), byte(r))
end

-- Aegisub utils-auto4.lua ile aynı sıra: stil rengi &HAABBGGRR, renk etiketi &HBBGGRR&,
-- saydamlık &HAA& (yalnızca a), sonra #RRGGBB[AA]; eşleşmezse nil
function util.extract_color(s)
  local a, b, g, r = s:match("&H(%x%x)(%x%x)(%x%x)(%x%x)")
  if a then return tonumber(r, 16), tonumber(g, 16), tonumber(b, 16), tonumber(a, 16) end
  b, g, r = s:match("&H(%x%x)(%x%x)(%x%x)&")
  if b then return tonumber(r, 16), tonumber(g, 16), tonumber(b, 16), 0 end
  a = s:match("&H(%x%x)&")
  if a then return 0, 0, 0, tonumber(a, 16) end
  r, g, b, a = s:match("#(%x%x)(%x?%x?)(%x?%x?)(%x?%x?)")
  if r then return tonumber(r, 16), tonumber(g, 16) or 0, tonumber(b, 16) or 0, tonumber(a, 16) or 0 end
  return nil
end

function util.alpha_from_style(s)
  local _, _, _, a = util.extract_color(s)
  return util.ass_alpha(a)
end

function util.color_from_style(s)
  local r, g, b = util.extract_color(s)
  return util.ass_color(r, g, b)
end

function util.interpolate(t, a, b)
  if t <= 0 then return a elseif t >= 1 then return b end
  return t * (b - a) + a
end

function util.interpolate_color(t, c1, c2)
  local r1, g1, b1 = util.extract_color(c1)
  local r2, g2, b2 = util.extract_color(c2)
  return util.ass_color(util.interpolate(t, r1, r2), util.interpolate(t, g1, g2), util.interpolate(t, b1, b2))
end

function util.interpolate_alpha(t, a1, a2)
  local _, _, _, x1 = util.extract_color(a1)
  local _, _, _, x2 = util.extract_color(a2)
  local function only(s)
    local h = s:match("&?[Hh](%x%x)&?$")
    return h and tonumber(h, 16)
  end
  x1 = (a1:match("^&?[Hh]%x%x&?$") and only(a1)) or x1
  x2 = (a2:match("^&?[Hh]%x%x&?$") and only(a2)) or x2
  return util.ass_alpha(util.interpolate(t, x1, x2))
end

-- Aegisub util.moon'dan birebir: H derece (|H| % 360), S/V 0–1; r,g,b YUVARLANMAMIŞ 0–255 döner.
function util.HSV_to_RGB(H, S, V)
  local r, g, b = 0, 0, 0
  if S == 0 then
    r = util.clamp(V * 255, 0, 255); g = r; b = r
  else
    H = math.abs(H) % 360
    local Hi = math.floor(H / 60)
    local f = H / 60.0 - Hi
    local p, q, t = V * (1 - S), V * (1 - f * S), V * (1 - (1 - f) * S)
    if Hi == 0 then r, g, b = V * 255.0, t * 255.0, p * 255.0
    elseif Hi == 1 then r, g, b = q * 255.0, V * 255.0, p * 255.0
    elseif Hi == 2 then r, g, b = p * 255.0, V * 255.0, t * 255.0
    elseif Hi == 3 then r, g, b = p * 255.0, q * 255.0, V * 255.0
    elseif Hi == 4 then r, g, b = t * 255.0, p * 255.0, V * 255.0
    elseif Hi == 5 then r, g, b = V * 255.0, p * 255.0, q * 255.0
    else error("math.floor(H % 360 / 60) should be [0, 6), is " .. tostring(Hi) .. "?") end
  end
  return r, g, b
end

-- Aegisub util.moon'dan birebir (S, L 0–1'e kıstırılır; tam sayı döner).
function util.HSL_to_RGB(H, S, L)
  local r, g, b
  H = math.abs(H) % 360
  S = util.clamp(S, 0, 1)
  L = util.clamp(L, 0, 1)
  if S == 0 then
    r, g, b = L, L, L
  else
    local Q = L < 0.5 and L * (1.0 + S) or L + S - (L * S)
    local P = 2.0 * L - Q
    local Hk = H / 360
    local Tr, Tg, Tb
    if Hk < 1/3 then Tr, Tg, Tb = Hk + 1/3, Hk, Hk + 2/3
    elseif Hk > 2/3 then Tr, Tg, Tb = Hk - 2/3, Hk, Hk - 1/3
    else Tr, Tg, Tb = Hk + 1/3, Hk, Hk - 1/3 end
    local function comp(T)
      if T < 1/6 then return P + ((Q - P) * 6.0 * T)
      elseif 1/6 <= T and T < 1/2 then return Q
      elseif 1/2 <= T and T < 2/3 then return P + ((Q - P) * (2/3 - T) * 6.0)
      else return P end
    end
    r, g, b = comp(Tr), comp(Tg), comp(Tb)
  end
  return math.floor(r * 255 + 0.5), math.floor(g * 255 + 0.5), math.floor(b * 255 + 0.5)
end

function util.RGB_to_HSV(r, g, b)
  r, g, b = r / 255, g / 255, b / 255
  local mx, mn = math.max(r, g, b), math.min(r, g, b)
  local d = mx - mn
  local h
  if d == 0 then h = 0
  elseif mx == r then h = ((g - b) / d) % 6
  elseif mx == g then h = (b - r) / d + 2
  else h = (r - g) / d + 4 end
  return h * 60, mx == 0 and 0 or d / mx, mx
end

function util.RGB_to_HSL(r, g, b)
  r, g, b = r / 255, g / 255, b / 255
  local mx, mn = math.max(r, g, b), math.min(r, g, b)
  local d = mx - mn
  local l = (mx + mn) / 2
  local h
  if d == 0 then h = 0
  elseif mx == r then h = ((g - b) / d) % 6
  elseif mx == g then h = (b - r) / d + 2
  else h = (r - g) / d + 4 end
  local s = d == 0 and 0 or d / (1 - math.abs(2 * l - 1))
  return h * 60, s, l
end

function util.uuid()
  local t = {}
  for i = 1, 32 do t[i] = string.format("%x", math.random(0, 15)) end
  local s = table.concat(t)
  return s:sub(1, 8) .. "-" .. s:sub(9, 12) .. "-4" .. s:sub(14, 16) .. "-a" .. s:sub(18, 20) .. "-" .. s:sub(21, 32)
end

-- Aegisub util.moon: trim gsub'un iki değerini döner; words headtail üzerine kurulu ("" başta olabilir)
function string.trim(s)
  return s:gsub("^%s*(.-)%s*$", "%1")
end

function string.headtail(s)
  local a, b, head, tail = s:find("(.-)%s+(.*)")
  if a then return head, tail end
  return s, ""
end

function string.words(s)
  return function()
    if s == "" then return end
    local head, tail = string.headtail(s)
    s = tail
    return head
  end
end

-- Aegisub include/utils.lua uyumluluğu: eski betiklerin kullandığı genel takma adlar.
util.trim, util.headtail, util.words = string.trim, string.headtail, string.words
table.copy = util.copy
copy_line = util.copy
table.copy_deep = util.deep_copy
ass_color = util.ass_color
ass_alpha = util.ass_alpha
ass_style_color = util.ass_style_color
extract_color = util.extract_color
alpha_from_style = util.alpha_from_style
color_from_style = util.color_from_style
HSV_to_RGB = util.HSV_to_RGB
HSL_to_RGB = util.HSL_to_RGB
clamp = util.clamp
interpolate = util.interpolate
interpolate_color = util.interpolate_color
interpolate_alpha = util.interpolate_alpha
