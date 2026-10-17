-- aegisub.util — Aegisub'ın util.moon dosyasının düz Lua karşılığı.
-- Özgün: Copyright (c) 2013, Thomas Goyne <plorkyeran@aegisub.org> (ISC lisansı)
-- Permission to use, copy, modify, and distribute this software for any
-- purpose with or without fee is hereby granted, provided that the above
-- copyright notice and this permission notice appear in all copies.

local sformat = string.format

local function check_table(v, fname)
  if type(v) ~= "table" then
    error(sformat("%s: tablo bekleniyordu, %s verildi", fname, type(v)), 3)
  end
end

local function copy(tbl)
  check_table(tbl, "copy")
  local r = {}
  for k, v in pairs(tbl) do r[k] = v end
  return r
end

local function deep_copy(tbl)
  check_table(tbl, "deep_copy")
  local seen = {}
  local function cp(val)
    if type(val) ~= "table" then return val end
    if seen[val] then return seen[val] end
    local result = {}
    seen[val] = result
    for k, v in pairs(val) do result[k] = cp(v) end
    return result
  end
  return cp(tbl)
end

local function ass_color(r, g, b) return sformat("&H%02X%02X%02X&", b, g, r) end
local function ass_alpha(a) return sformat("&H%02X&", a) end
local function ass_style_color(r, g, b, a) return sformat("&H%02X%02X%02X%02X", a, b, g, r) end

local function extract_color(s)
  if type(s) ~= "string" then error("extract_color: metin bekleniyordu", 2) end
  local a, b, g, r = s:match('&H(%x%x)(%x%x)(%x%x)(%x%x)')
  if a then
    return tonumber(r, 16), tonumber(g, 16), tonumber(b, 16), tonumber(a, 16)
  end
  b, g, r = s:match('&H(%x%x)(%x%x)(%x%x)&')
  if b then
    return tonumber(r, 16), tonumber(g, 16), tonumber(b, 16), 0
  end
  a = s:match('&H(%x%x)&')
  if a then
    return 0, 0, 0, tonumber(a, 16)
  end
  local r2, g2, b2, a2 = s:match('#(%x%x)(%x?%x?)(%x?%x?)(%x?%x?)')
  if r2 then
    return tonumber(r2, 16), tonumber(g2, 16) or 0, tonumber(b2, 16) or 0, tonumber(a2, 16) or 0
  end
end

local function alpha_from_style(scolor)
  return ass_alpha(select(4, extract_color(scolor)))
end

local function color_from_style(scolor)
  local r, g, b = extract_color(scolor)
  return ass_color(r or 0, g or 0, b or 0)
end

local function clamp(val, min, max)
  if val < min then return min elseif val > max then return max else return val end
end

local function HSV_to_RGB(H, S, V)
  local r, g, b = 0, 0, 0
  if S == 0 then
    r = clamp(V * 255, 0, 255); g = r; b = r
  else
    H = math.abs(H) % 360
    local Hi = math.floor(H / 60)
    local f = H / 60.0 - Hi
    local p = V * (1 - S)
    local q = V * (1 - f * S)
    local t = V * (1 - (1 - f) * S)
    if Hi == 0 then r, g, b = V * 255.0, t * 255.0, p * 255.0
    elseif Hi == 1 then r, g, b = q * 255.0, V * 255.0, p * 255.0
    elseif Hi == 2 then r, g, b = p * 255.0, V * 255.0, t * 255.0
    elseif Hi == 3 then r, g, b = p * 255.0, q * 255.0, V * 255.0
    elseif Hi == 4 then r, g, b = t * 255.0, p * 255.0, V * 255.0
    elseif Hi == 5 then r, g, b = V * 255.0, p * 255.0, q * 255.0
    else error("math.floor(H % 360 / 60) should be [0, 6), is " .. Hi .. "?") end
  end
  return r, g, b
end

local function HSL_to_RGB(H, S, L)
  local r, g, b
  H = math.abs(H) % 360
  S = clamp(S, 0, 1)
  L = clamp(L, 0, 1)
  if S == 0 then
    r, g, b = L, L, L
  else
    local Q
    if L < 0.5 then Q = L * (1.0 + S) else Q = L + S - (L * S) end
    local P = 2.0 * L - Q
    local Hk = H / 360
    local Tr, Tg, Tb
    if Hk < 1 / 3 then
      Tr, Tg, Tb = Hk + 1 / 3, Hk, Hk + 2 / 3
    elseif Hk > 2 / 3 then
      Tr, Tg, Tb = Hk - 2 / 3, Hk, Hk - 1 / 3
    else
      Tr, Tg, Tb = Hk + 1 / 3, Hk, Hk - 1 / 3
    end
    local function get_component(T)
      if T < 1 / 6 then return P + ((Q - P) * 6.0 * T)
      elseif 1 / 6 <= T and T < 1 / 2 then return Q
      elseif 1 / 2 <= T and T < 2 / 3 then return P + ((Q - P) * (2 / 3 - T) * 6.0)
      else return P end
    end
    r, g, b = get_component(Tr), get_component(Tg), get_component(Tb)
  end
  return math.floor(r * 255 + 0.5), math.floor(g * 255 + 0.5), math.floor(b * 255 + 0.5)
end

local function trim(s) return (s:gsub('^%s*(.-)%s*$', '%1')) end

local function headtail(s)
  local a, b, head, tail = s:find('(.-)%s+(.*)')
  if a then return head, tail else return s, '' end
end

local function words(s)
  return function()
    if s == '' then return end
    local head, tail = headtail(s)
    s = tail
    return head
  end
end

local function interpolate(pct, min, max)
  if pct <= 0 then return min elseif pct >= 1 then return max else return pct * (max - min) + min end
end

local function interpolate_color(pct, first, last)
  local r1, g1, b1 = extract_color(first)
  local r2, g2, b2 = extract_color(last)
  local r, g, b = interpolate(pct, r1, r2), interpolate(pct, g1, g2), interpolate(pct, b1, b2)
  return ass_color(r, g, b)
end

local function interpolate_alpha(pct, first, last)
  return ass_alpha(interpolate(pct, select(4, extract_color(first)), select(4, extract_color(last))))
end

return {
  copy = copy, deep_copy = deep_copy, ass_color = ass_color, ass_alpha = ass_alpha,
  ass_style_color = ass_style_color, extract_color = extract_color,
  alpha_from_style = alpha_from_style, color_from_style = color_from_style,
  HSV_to_RGB = HSV_to_RGB, HSL_to_RGB = HSL_to_RGB, trim = trim, headtail = headtail,
  words = words, clamp = clamp, interpolate = interpolate,
  interpolate_color = interpolate_color, interpolate_alpha = interpolate_alpha,
}
