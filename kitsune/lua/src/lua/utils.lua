-- Aegisub "utils.lua" uyumlu yardımcılar (Kitsune).
local util = {}

function util.clamp(v, lo, hi) return math.max(lo, math.min(hi, v)) end

function util.copy(src)
  local t = {}
  for k, v in pairs(src) do t[k] = v end
  return t
end

function util.deep_copy(src)
  local seen = {}
  local function cp(o)
    if type(o) ~= "table" then return o end
    if seen[o] then return seen[o] end
    local t = {}
    seen[o] = t
    for k, v in pairs(o) do t[cp(k)] = cp(v) end
    return setmetatable(t, getmetatable(o))
  end
  return cp(src)
end

function util.ass_color(r, g, b) return string.format("&H%02X%02X%02X&", b, g, r) end
function util.ass_alpha(a) return string.format("&H%02X&", a) end
function util.ass_style_color(r, g, b, a) return string.format("&H%02X%02X%02X%02X", a or 0, b, g, r) end

function util.extract_color(s)
  local a, b, g, r
  a, b, g, r = s:match("&H(%x%x)(%x%x)(%x%x)(%x%x)&?")
  if a then return tonumber(r, 16), tonumber(g, 16), tonumber(b, 16), tonumber(a, 16) end
  b, g, r = s:match("&H(%x%x)(%x%x)(%x%x)&?")
  if b then return tonumber(r, 16), tonumber(g, 16), tonumber(b, 16), 0 end
  r, g, b = s:match("#(%x%x)(%x%x)(%x%x)")
  if r then return tonumber(r, 16), tonumber(g, 16), tonumber(b, 16), 0 end
  a = s:match("&H(%x%x)&?")
  if a then return 0, 0, 0, tonumber(a, 16) end
  return 0, 0, 0, 0
end

function util.alpha_from_style(s) local _, _, _, a = util.extract_color(s); return util.ass_alpha(a) end
function util.color_from_style(s) local r, g, b = util.extract_color(s); return util.ass_color(r, g, b) end

function util.interpolate(pct, a, b) return a + (b - a) * pct end

function util.interpolate_color(pct, c1, c2)
  local r1, g1, b1 = util.extract_color(c1)
  local r2, g2, b2 = util.extract_color(c2)
  return util.ass_color(math.floor(util.interpolate(pct, r1, r2) + 0.5),
    math.floor(util.interpolate(pct, g1, g2) + 0.5), math.floor(util.interpolate(pct, b1, b2) + 0.5))
end

function util.interpolate_alpha(pct, a1, a2)
  local _, _, _, x = util.extract_color(a1)
  local _, _, _, y = util.extract_color(a2)
  return util.ass_alpha(math.floor(util.interpolate(pct, x, y) + 0.5))
end

function util.RGB_to_HSV(r, g, b)
  r, g, b = r / 255, g / 255, b / 255
  local mx, mn = math.max(r, g, b), math.min(r, g, b)
  local d = mx - mn
  local h = 0
  if d ~= 0 then
    if mx == r then h = ((g - b) / d) % 6
    elseif mx == g then h = (b - r) / d + 2
    else h = (r - g) / d + 4 end
    h = h * 60
  end
  return h, mx == 0 and 0 or d / mx * 255, mx * 255
end

function util.HSV_to_RGB(h, s, v)
  s, v = s / 255, v / 255
  local c = v * s
  local x = c * (1 - math.abs((h / 60) % 2 - 1))
  local m = v - c
  local r, g, b
  h = h % 360
  if h < 60 then r, g, b = c, x, 0 elseif h < 120 then r, g, b = x, c, 0
  elseif h < 180 then r, g, b = 0, c, x elseif h < 240 then r, g, b = 0, x, c
  elseif h < 300 then r, g, b = x, 0, c else r, g, b = c, 0, x end
  local function f(n) return math.floor((n + m) * 255 + 0.5) end
  return f(r), f(g), f(b)
end

function util.RGB_to_HSL(r, g, b)
  r, g, b = r / 255, g / 255, b / 255
  local mx, mn = math.max(r, g, b), math.min(r, g, b)
  local l = (mx + mn) / 2
  local h, s = 0, 0
  local d = mx - mn
  if d ~= 0 then
    s = d / (1 - math.abs(2 * l - 1))
    if mx == r then h = ((g - b) / d) % 6 elseif mx == g then h = (b - r) / d + 2 else h = (r - g) / d + 4 end
    h = h * 60
  end
  return h, s * 255, l * 255
end

function util.HSL_to_RGB(h, s, l)
  s, l = s / 255, l / 255
  local c = (1 - math.abs(2 * l - 1)) * s
  local x = c * (1 - math.abs((h / 60) % 2 - 1))
  local m = l - c / 2
  local r, g, b
  h = h % 360
  if h < 60 then r, g, b = c, x, 0 elseif h < 120 then r, g, b = x, c, 0
  elseif h < 180 then r, g, b = 0, c, x elseif h < 240 then r, g, b = 0, x, c
  elseif h < 300 then r, g, b = x, 0, c else r, g, b = c, 0, x end
  local function f(n) return math.floor((n + m) * 255 + 0.5) end
  return f(r), f(g), f(b)
end

function util.fix_fnt(tag)
  return function(sub) return sub:gsub("\\fn%s*([^\\}]+)", tag) end
end

function string.headtail(s)
  local a, b = s:match("^(.-)%s+(.*)$")
  if a then return a, b end
  return s, ""
end

function string.trim(s) return (s:gsub("^%s+", ""):gsub("%s+$", "")) end

function string.words(s)
  return coroutine.wrap(function()
    for w in s:gmatch("%S+") do coroutine.yield(w) end
  end)
end

function math.round(n, p)
  p = p or 0
  local m = 10 ^ p
  return math.floor(n * m + 0.5) / m
end

table.copy = util.copy
table.copy_deep = util.deep_copy

return util
