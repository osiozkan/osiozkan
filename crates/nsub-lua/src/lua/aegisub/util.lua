-- aegisub.util: port of Aegisub's util.moon (Copyright (c) 2005-2013 Aegisub contributors, ISC licence).
local error, pairs, select, sformat, tonumber, type = error, pairs, select, string.format, tonumber, type

local M = {}

local function clamp(val, min, max)
  if val < min then return min elseif val > max then return max else return val end
end

function M.copy(tbl)
  local r = {}
  for k, v in pairs(tbl) do r[k] = v end
  return r
end

function M.deep_copy(tbl)
  local seen = {}
  local function copy(val)
    if type(val) ~= "table" then return val end
    if seen[val] then return seen[val] end
    local r = {}
    seen[val] = r
    for k, v in pairs(val) do r[k] = copy(v) end
    return r
  end
  return copy(tbl)
end

function M.ass_color(r, g, b) return sformat("&H%02X%02X%02X&", b, g, r) end
function M.ass_alpha(a) return sformat("&H%02X&", a) end
function M.ass_style_color(r, g, b, a) return sformat("&H%02X%02X%02X%02X", a, b, g, r) end

function M.extract_color(s)
  local a, b, g, r = s:match("&H(%x%x)(%x%x)(%x%x)(%x%x)")
  if a then return tonumber(r, 16), tonumber(g, 16), tonumber(b, 16), tonumber(a, 16) end
  b, g, r = s:match("&H(%x%x)(%x%x)(%x%x)&")
  if b then return tonumber(r, 16), tonumber(g, 16), tonumber(b, 16), 0 end
  a = s:match("&H(%x%x)&")
  if a then return 0, 0, 0, tonumber(a, 16) end
  r, g, b, a = s:match("#(%x%x)(%x?%x?)(%x?%x?)(%x?%x?)")
  if r then return tonumber(r, 16), tonumber(g, 16) or 0, tonumber(b, 16) or 0, tonumber(a, 16) or 0 end
end

function M.alpha_from_style(scolor) return M.ass_alpha(select(4, M.extract_color(scolor))) end
function M.color_from_style(scolor)
  local r, g, b = M.extract_color(scolor)
  return M.ass_color(r or 0, g or 0, b or 0)
end

function M.HSV_to_RGB(H, S, V)
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
    else error("math.floor(H % 360 / 60) should be [0, 6), is " .. tostring(Hi) .. "?") end
  end
  return r, g, b
end

function M.HSL_to_RGB(H, S, L)
  H = math.abs(H) % 360
  S = clamp(S, 0, 1)
  L = clamp(L, 0, 1)
  local r, g, b
  if S == 0 then
    r, g, b = L, L, L
  else
    local Q
    if L < 0.5 then Q = L * (1.0 + S) else Q = L + S - (L * S) end
    local P = 2.0 * L - Q
    local Hk = H / 360
    local Tr, Tg, Tb
    if Hk < 1 / 3 then Tr, Tg, Tb = Hk + 1 / 3, Hk, Hk + 2 / 3
    elseif Hk > 2 / 3 then Tr, Tg, Tb = Hk - 2 / 3, Hk, Hk - 1 / 3
    else Tr, Tg, Tb = Hk + 1 / 3, Hk, Hk - 1 / 3 end
    local function comp(T)
      if T < 1 / 6 then return P + ((Q - P) * 6.0 * T)
      elseif 1 / 6 <= T and T < 1 / 2 then return Q
      elseif 1 / 2 <= T and T < 2 / 3 then return P + ((Q - P) * (2 / 3 - T) * 6.0)
      else return P end
    end
    r, g, b = comp(Tr), comp(Tg), comp(Tb)
  end
  return math.floor(r * 255 + 0.5), math.floor(g * 255 + 0.5), math.floor(b * 255 + 0.5)
end

function M.trim(s) return (s:gsub("^%s*(.-)%s*$", "%1")) end

function M.headtail(s)
  local a, b, head, tail = s:find("(.-)%s+(.*)")
  if a then return head, tail else return s, "" end
end

function M.words(s)
  return function()
    if s == "" then return end
    local head, tail = M.headtail(s)
    s = tail
    return head
  end
end

M.clamp = clamp

function M.interpolate(pct, min, max)
  if pct <= 0 then return min elseif pct >= 1 then return max else return pct * (max - min) + min end
end

function M.interpolate_color(pct, first, last)
  local r1, g1, b1 = M.extract_color(first)
  local r2, g2, b2 = M.extract_color(last)
  return M.ass_color(M.interpolate(pct, r1, r2), M.interpolate(pct, g1, g2), M.interpolate(pct, b1, b2))
end

function M.interpolate_alpha(pct, first, last)
  return M.ass_alpha(M.interpolate(pct, select(4, M.extract_color(first)), select(4, M.extract_color(last))))
end

return M
