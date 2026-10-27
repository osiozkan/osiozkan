-- kaku util.lua: Aegisub-compatible helper library (clean-room implementation)
local util = {}

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
  return setmetatable(r, getmetatable(t))
end

function util.clamp(v, lo, hi) if v < lo then return lo elseif v > hi then return hi end return v end

local function clamp255(v) v = math.floor(v + 0.5); if v < 0 then return 0 elseif v > 255 then return 255 end return v end

function util.ass_color(r, g, b) return string.format("&H%02X%02X%02X&", clamp255(b), clamp255(g), clamp255(r)) end
function util.ass_style_color(r, g, b, a) return string.format("&H%02X%02X%02X%02X", clamp255(a or 0), clamp255(b), clamp255(g), clamp255(r)) end

function util.extract_color(s)
  local a, b, g, r
  if type(s) ~= "string" then return 0, 0, 0, 0 end
  local hex = s:match("&?[Hh](%x+)&?")
  if hex then
    hex = string.rep("0", 8 - #hex) .. hex
    a, b, g, r = hex:match("(%x%x)(%x%x)(%x%x)(%x%x)")
    return tonumber(r, 16), tonumber(g, 16), tonumber(b, 16), tonumber(a, 16)
  end
  local n = tonumber(s)
  if n then
    n = n % 4294967296
    return n % 256, math.floor(n / 256) % 256, math.floor(n / 65536) % 256, math.floor(n / 16777216) % 256
  end
  return 0, 0, 0, 0
end

function util.alpha_from_style(s) local _, _, _, a = util.extract_color(s); return string.format("&H%02X&", a) end
function util.color_from_style(s) local r, g, b = util.extract_color(s); return util.ass_color(r, g, b) end

function util.interpolate(t, a, b)
  if t <= 0 then return a elseif t >= 1 then return b end
  return a + (b - a) * t
end

function util.interpolate_color(t, c1, c2)
  local r1, g1, b1 = util.extract_color(c1)
  local r2, g2, b2 = util.extract_color(c2)
  return util.ass_color(util.interpolate(t, r1, r2), util.interpolate(t, g1, g2), util.interpolate(t, b1, b2))
end

function util.interpolate_alpha(t, a1, a2)
  local _, _, _, x = util.extract_color(a1)
  local _, _, _, y = util.extract_color(a2)
  return string.format("&H%02X&", clamp255(util.interpolate(t, x, y)))
end

function util.RGB_to_HSV(r, g, b)
  r, g, b = r / 255, g / 255, b / 255
  local mx, mn = math.max(r, g, b), math.min(r, g, b)
  local d, h = mx - mn, 0
  if d > 0 then
    if mx == r then h = ((g - b) / d) % 6 elseif mx == g then h = (b - r) / d + 2 else h = (r - g) / d + 4 end
    h = h * 60
  end
  return h, (mx == 0) and 0 or d / mx, mx
end

function util.HSV_to_RGB(h, s, v)
  h = h % 360
  local c = v * s
  local x = c * (1 - math.abs((h / 60) % 2 - 1))
  local m = v - c
  local r, g, b
  if h < 60 then r, g, b = c, x, 0 elseif h < 120 then r, g, b = x, c, 0 elseif h < 180 then r, g, b = 0, c, x
  elseif h < 240 then r, g, b = 0, x, c elseif h < 300 then r, g, b = x, 0, c else r, g, b = c, 0, x end
  return clamp255((r + m) * 255), clamp255((g + m) * 255), clamp255((b + m) * 255)
end

function util.HSL_to_RGB(h, s, l)
  local c = (1 - math.abs(2 * l - 1)) * s
  local v = l + c / 2
  local sv = (v == 0) and 0 or 2 * (1 - l / v)
  return util.HSV_to_RGB(h, sv, v)
end

function util.time_to_cs(ms) return math.floor(ms / 10 + 0.5) end

-- String additions (Aegisub adds these to the string table).
function string.headtail(s)
  local a, b = s:match("^%s*(%S+)%s*(.*)$")
  if a then return a, b end
  return "", ""
end
function string.trim(s) return (s:gsub("^%s+", ""):gsub("%s+$", "")) end
function string.words(s) return s:gmatch("%S+") end

function table.copy(t) return util.copy(t) end
function table.copy_deep(t) return util.deep_copy(t) end
function table.append(t, v) t[#t + 1] = v end
function io.fileexists(p) local f = io.open(p, "rb"); if f then f:close() return true end return false end

return util
