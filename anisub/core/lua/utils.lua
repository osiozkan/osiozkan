-- AniSub utils.lua : compatible with Aegisub's utils.lua API.
util = util or {}

function math.round(n) return math.floor(n + 0.5) end
if not math.clamp then function math.clamp(v, lo, hi) return math.max(lo, math.min(hi, v)) end end

function table.copy(t)
  local r = {}
  for k, v in pairs(t) do r[k] = v end
  return r
end
function table.copy_deep(t)
  if type(t) ~= "table" then return t end
  local r = {}
  for k, v in pairs(t) do r[k] = table.copy_deep(v) end
  return setmetatable(r, getmetatable(t))
end
function table.values(t)
  local r = {}
  for _, v in pairs(t) do r[#r + 1] = v end
  return r
end
function table.keys(t)
  local r = {}
  for k in pairs(t) do r[#r + 1] = k end
  return r
end

function string.headtail(s)
  local a, b = s:find("%s+")
  if a then return s:sub(1, a - 1), s:sub(b + 1) end
  return s, ""
end
function string.words(s)
  return function()
    local h, t = string.headtail(s)
    if h == "" then return nil end
    s = t
    return h
  end
end
function string.trim(s) return (s:gsub("^%s+", ""):gsub("%s+$", "")) end
function string.ltrim(s) return (s:gsub("^%s+", "")) end
function string.rtrim(s) return (s:gsub("%s+$", "")) end
function string.split(s, sep)
  local r, i = {}, 1
  while true do
    local a, b = s:find(sep, i, true)
    if not a then r[#r + 1] = s:sub(i); break end
    r[#r + 1] = s:sub(i, a - 1)
    i = b + 1
  end
  return r
end

function util.copy(t) return table.copy(t) end
util.deep_copy = table.copy_deep
function util.clamp(v, lo, hi) return math.max(lo, math.min(hi, v)) end
function util.interpolate(t, a, b) return a + (b - a) * t end

function util.ass_color(r, g, b)
  return string.format("&H%02X%02X%02X&", util.clamp(math.floor(b + 0.5), 0, 255), util.clamp(math.floor(g + 0.5), 0, 255), util.clamp(math.floor(r + 0.5), 0, 255))
end
function util.ass_alpha(a) return string.format("&H%02X&", util.clamp(math.floor(a + 0.5), 0, 255)) end
function util.ass_style_color(r, g, b, a)
  return string.format("&H%02X%02X%02X%02X", util.clamp(math.floor((a or 0) + 0.5), 0, 255), util.clamp(math.floor(b + 0.5), 0, 255), util.clamp(math.floor(g + 0.5), 0, 255), util.clamp(math.floor(r + 0.5), 0, 255))
end

function util.extract_color(s)
  local a, b, g, r
  s = tostring(s)
  local m = s:match("^&?[Hh]?(%x+)&?$")
  if m then
    local v = tonumber(m, 16)
    if #m > 6 then
      a = math.floor(v / 16777216) % 256
    else
      a = 0
    end
    b = math.floor(v / 65536) % 256
    g = math.floor(v / 256) % 256
    r = v % 256
    return r, g, b, a
  end
  local n = tonumber(s)
  if n then
    return n % 256, math.floor(n / 256) % 256, math.floor(n / 65536) % 256, math.floor(n / 16777216) % 256
  end
  return 0, 0, 0, 0
end
function util.alpha_from_style(s)
  local _, _, _, a = util.extract_color(s)
  return util.ass_alpha(a)
end
function util.color_from_style(s)
  local r, g, b = util.extract_color(s)
  return util.ass_color(r, g, b)
end

function util.interpolate_color(t, c1, c2)
  local r1, g1, b1 = util.extract_color(c1)
  local r2, g2, b2 = util.extract_color(c2)
  return util.ass_color(util.interpolate(t, r1, r2), util.interpolate(t, g1, g2), util.interpolate(t, b1, b2))
end
function util.interpolate_alpha(t, a1, a2)
  local _, _, _, x = util.extract_color(a1)
  local _, _, _, y = util.extract_color(a2)
  return util.ass_alpha(util.interpolate(t, x, y))
end

function util.rgb_to_hsl(r, g, b)
  r, g, b = r / 255, g / 255, b / 255
  local mx, mn = math.max(r, g, b), math.min(r, g, b)
  local h, s, l = 0, 0, (mx + mn) / 2
  if mx ~= mn then
    local d = mx - mn
    s = l > 0.5 and d / (2 - mx - mn) or d / (mx + mn)
    if mx == r then h = (g - b) / d + (g < b and 6 or 0)
    elseif mx == g then h = (b - r) / d + 2
    else h = (r - g) / d + 4 end
    h = h / 6
  end
  return h * 255, s * 255, l * 255
end
function util.hsl_to_rgb(h, s, l)
  h, s, l = h / 255, s / 255, l / 255
  if s == 0 then local v = l * 255; return v, v, v end
  local function hue(p, q, t)
    if t < 0 then t = t + 1 end
    if t > 1 then t = t - 1 end
    if t < 1 / 6 then return p + (q - p) * 6 * t end
    if t < 1 / 2 then return q end
    if t < 2 / 3 then return p + (q - p) * (2 / 3 - t) * 6 end
    return p
  end
  local q = l < 0.5 and l * (1 + s) or l + s - l * s
  local p = 2 * l - q
  return hue(p, q, h + 1 / 3) * 255, hue(p, q, h) * 255, hue(p, q, h - 1 / 3) * 255
end
function util.HSV_to_RGB(h, s, v)
  h, s, v = h / 255 * 6, s / 255, v / 255
  local i = math.floor(h)
  local f = h - i
  local p, q, t = v * (1 - s), v * (1 - s * f), v * (1 - s * (1 - f))
  local r, g, b
  i = i % 6
  if i == 0 then r, g, b = v, t, p elseif i == 1 then r, g, b = q, v, p elseif i == 2 then r, g, b = p, v, t
  elseif i == 3 then r, g, b = p, q, v elseif i == 4 then r, g, b = t, p, v else r, g, b = v, p, q end
  return r * 255, g * 255, b * 255
end
util.HSL_to_RGB = util.hsl_to_rgb
util.RGB_to_HSL = util.rgb_to_hsl
function util.RGB_to_HSV(r, g, b)
  r, g, b = r / 255, g / 255, b / 255
  local mx, mn = math.max(r, g, b), math.min(r, g, b)
  local d = mx - mn
  local h = 0
  if d ~= 0 then
    if mx == r then h = ((g - b) / d) % 6 elseif mx == g then h = (b - r) / d + 2 else h = (r - g) / d + 4 end
    h = h / 6
  end
  return h * 255, (mx == 0 and 0 or d / mx) * 255, mx * 255
end

function util.uuid()
  local t = {}
  for i = 1, 16 do t[i] = string.format("%02x", math.random(0, 255)) end
  return table.concat(t)
end

function util.trim(s) return string.trim(s) end
function util.split(s, sep) return string.split(s, sep) end
