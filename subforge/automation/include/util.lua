-- aegisub.util compatible helpers
local util = {}

local function clamp(v, lo, hi) if v < lo then return lo elseif v > hi then return hi end return v end
util.clamp = clamp

function util.uuid()
  local t = {}
  for i = 1, 32 do t[i] = string.format("%x", math.random(0, 15)) end
  local s = table.concat(t)
  return s:sub(1,8).."-"..s:sub(9,12).."-4"..s:sub(14,16).."-a"..s:sub(18,20).."-"..s:sub(21,32)
end

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

function util.ass_color(r, g, b)
  return string.format("&H%02X%02X%02X&", clamp(math.floor(b+0.5),0,255), clamp(math.floor(g+0.5),0,255), clamp(math.floor(r+0.5),0,255))
end
function util.ass_alpha(a) return string.format("&H%02X&", clamp(math.floor(a+0.5),0,255)) end
function util.ass_style_color(r, g, b, a)
  return string.format("&H%02X%02X%02X%02X", clamp(math.floor((a or 0)+0.5),0,255), clamp(math.floor(b+0.5),0,255), clamp(math.floor(g+0.5),0,255), clamp(math.floor(r+0.5),0,255))
end

function util.extract_color(s)
  s = tostring(s)
  local a, b, g, r
  local h = s:match("^%s*&?[Hh](%x+)&?%s*$")
  if h then
    if #h <= 6 then h = string.rep("0", 6 - #h) .. h; a = 0 else h = string.rep("0", 8 - #h) .. h; a = tonumber(h:sub(1,2), 16); h = h:sub(3) end
    b, g, r = tonumber(h:sub(1,2),16), tonumber(h:sub(3,4),16), tonumber(h:sub(5,6),16)
    return r, g, b, a
  end
  local hex = s:match("^%s*#(%x%x%x%x%x%x)%s*$")
  if hex then
    return tonumber(hex:sub(1,2),16), tonumber(hex:sub(3,4),16), tonumber(hex:sub(5,6),16), 0
  end
  local n = tonumber(s)
  if n then
    n = n % 4294967296
    return n % 256, math.floor(n / 256) % 256, math.floor(n / 65536) % 256, math.floor(n / 16777216) % 256
  end
  return 0, 0, 0, 0
end

function util.alpha_from_style(c)
  local _, _, _, a = util.extract_color(c)
  return util.ass_alpha(a)
end
function util.color_from_style(c)
  local r, g, b = util.extract_color(c)
  return util.ass_color(r, g, b)
end

function util.interpolate(pct, a, b)
  if pct <= 0 then return a elseif pct >= 1 then return b end
  return a + (b - a) * pct
end
function util.interpolate_color(pct, c1, c2)
  local r1, g1, b1 = util.extract_color(c1)
  local r2, g2, b2 = util.extract_color(c2)
  return util.ass_color(util.interpolate(pct, r1, r2), util.interpolate(pct, g1, g2), util.interpolate(pct, b1, b2))
end
function util.interpolate_alpha(pct, a1, a2)
  local _, _, _, x1 = util.extract_color(a1)
  local _, _, _, x2 = util.extract_color(a2)
  return util.ass_alpha(util.interpolate(pct, x1, x2))
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
  h = h * 60
  return h % 360, mx == 0 and 0 or d / mx, mx
end
function util.HSV_to_RGB(h, s, v)
  h = h % 360
  local c = v * s
  local x = c * (1 - math.abs((h / 60) % 2 - 1))
  local m = v - c
  local r, g, b
  if h < 60 then r, g, b = c, x, 0 elseif h < 120 then r, g, b = x, c, 0
  elseif h < 180 then r, g, b = 0, c, x elseif h < 240 then r, g, b = 0, x, c
  elseif h < 300 then r, g, b = x, 0, c else r, g, b = c, 0, x end
  return math.floor((r+m)*255+0.5), math.floor((g+m)*255+0.5), math.floor((b+m)*255+0.5)
end
function util.RGB_to_HSL(r, g, b)
  r, g, b = r/255, g/255, b/255
  local mx, mn = math.max(r,g,b), math.min(r,g,b)
  local l = (mx + mn) / 2
  if mx == mn then return 0, 0, l end
  local d = mx - mn
  local s = l > 0.5 and d / (2 - mx - mn) or d / (mx + mn)
  local h
  if mx == r then h = (g - b) / d + (g < b and 6 or 0) elseif mx == g then h = (b - r) / d + 2 else h = (r - g) / d + 4 end
  return h * 60, s, l
end
function util.HSL_to_RGB(h, s, l)
  h = (h % 360) / 360
  local function hue(p, q, t)
    if t < 0 then t = t + 1 elseif t > 1 then t = t - 1 end
    if t < 1/6 then return p + (q - p) * 6 * t elseif t < 1/2 then return q elseif t < 2/3 then return p + (q - p) * (2/3 - t) * 6 end
    return p
  end
  if s == 0 then local v = math.floor(l*255+0.5) return v, v, v end
  local q = l < 0.5 and l * (1 + s) or l + s - l * s
  local p = 2 * l - q
  return math.floor(hue(p,q,h+1/3)*255+0.5), math.floor(hue(p,q,h)*255+0.5), math.floor(hue(p,q,h-1/3)*255+0.5)
end

function util.trim(s) return (s:gsub("^%s*(.-)%s*$", "%1")) end
function util.headtail(s)
  local a, b, head, tail = s:find("^(%S*)%s*(.*)$")
  return head, tail
end
function util.words(s)
  return s:gmatch("%S+")
end

return util
