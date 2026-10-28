-- utils.lua: helpers compatible with Aegisub's include("utils.lua").
function string.headtail(s)
  local a, b, head, tail = s:find("(.-)[ \t]+(.*)")
  if a then return head, tail end
  return s, ""
end

function string.trim(s) return (s:gsub("^%s+", ""):gsub("%s+$", "")) end

function string.words(s)
  return coroutine.wrap(function()
    for w in s:gmatch("%S+") do coroutine.yield(w) end
  end)
end

function table.copy(old)
  local new = {}
  for k, v in pairs(old) do new[k] = v end
  return new
end

function table.copy_deep(old)
  if type(old) ~= "table" then return old end
  local new = {}
  for k, v in pairs(old) do new[k] = table.copy_deep(v) end
  return setmetatable(new, getmetatable(old))
end

function math.round(n, d)
  d = d or 0
  local m = 10 ^ d
  return math.floor(n * m + 0.5) / m
end

util = util or {}
util.copy = table.copy
util.deep_copy = table.copy_deep

function util.clamp(v, lo, hi) return math.max(lo, math.min(hi, v)) end

function util.ass_color(r, g, b) return string.format("&H%02X%02X%02X&", b, g, r) end
function util.ass_alpha(a) return string.format("&H%02X&", a) end
function util.ass_style_color(r, g, b, a)
  return string.format("&H%02X%02X%02X%02X", a or 0, b, g, r)
end

function util.extract_color(s)
  local a, b, g, r = 0, 0, 0, 0
  if type(s) ~= "string" then return 0, 0, 0, 0 end
  local hex = s:match("&[Hh](%x+)&?")
  if hex then
    hex = ("0"):rep(8 - #hex) .. hex
    a = tonumber(hex:sub(1, 2), 16); b = tonumber(hex:sub(3, 4), 16)
    g = tonumber(hex:sub(5, 6), 16); r = tonumber(hex:sub(7, 8), 16)
  end
  return r, g, b, a
end

function util.alpha_from_style(c) local _, _, _, a = util.extract_color(c); return util.ass_alpha(a) end
function util.color_from_style(c) local r, g, b = util.extract_color(c); return util.ass_color(r, g, b) end

function util.interpolate(t, a, b) return a + (b - a) * t end

function util.interpolate_color(t, c1, c2)
  local r1, g1, b1 = util.extract_color(c1)
  local r2, g2, b2 = util.extract_color(c2)
  return util.ass_color(
    math.floor(util.interpolate(t, r1, r2) + 0.5),
    math.floor(util.interpolate(t, g1, g2) + 0.5),
    math.floor(util.interpolate(t, b1, b2) + 0.5))
end

function util.interpolate_alpha(t, a1, a2)
  local x = tonumber(a1:match("&H(%x+)&?"), 16)
  local y = tonumber(a2:match("&H(%x+)&?"), 16)
  return util.ass_alpha(math.floor(util.interpolate(t, x, y) + 0.5))
end

function util.RGB_to_HSV(r, g, b)
  r, g, b = r / 255, g / 255, b / 255
  local mx, mn = math.max(r, g, b), math.min(r, g, b)
  local d, h = mx - mn, 0
  if d ~= 0 then
    if mx == r then h = ((g - b) / d) % 6 elseif mx == g then h = (b - r) / d + 2 else h = (r - g) / d + 4 end
    h = h * 60
  end
  return h, mx == 0 and 0 or d / mx, mx
end

function util.HSV_to_RGB(h, s, v)
  local c = v * s
  local x = c * (1 - math.abs((h / 60) % 2 - 1))
  local m = v - c
  local r, g, b = 0, 0, 0
  local i = math.floor(h / 60) % 6
  if i == 0 then r, g, b = c, x, 0 elseif i == 1 then r, g, b = x, c, 0
  elseif i == 2 then r, g, b = 0, c, x elseif i == 3 then r, g, b = 0, x, c
  elseif i == 4 then r, g, b = x, 0, c else r, g, b = c, 0, x end
  return math.floor((r + m) * 255 + 0.5), math.floor((g + m) * 255 + 0.5), math.floor((b + m) * 255 + 0.5)
end

function util.uuid()
  local t = {}
  for i = 1, 16 do t[i] = string.format("%02x", math.random(0, 255)) end
  return table.concat(t)
end

function util.fade(ms_in, ms_out, dur) return string.format("\\fad(%d,%d)", ms_in, ms_out) end
