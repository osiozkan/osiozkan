-- aegisub.util (Aegisub-compatible)
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

function util.clamp(v, lo, hi) return math.max(lo, math.min(hi, v)) end

local function clamp255(v) return math.max(0, math.min(255, math.floor(v + 0.5))) end

function util.ass_color(r, g, b)
  return string.format("&H%02X%02X%02X&", clamp255(b), clamp255(g), clamp255(r))
end
function util.ass_alpha(a) return string.format("&H%02X&", clamp255(a)) end
function util.ass_style_color(r, g, b, a)
  return string.format("&H%02X%02X%02X%02X", clamp255(a or 0), clamp255(b), clamp255(g), clamp255(r))
end

function util.extract_color(s)
  local a, b, g, r
  s = tostring(s)
  -- style form &HAABBGGRR  /  tag form &HBBGGRR&  /  alpha &HAA&  / decimal
  local v = s:match("^&[Hh](%x+)&?$")
  if v then
    local n = tonumber(v, 16)
    if #v <= 2 then return n, nil, nil, nil end
    a = math.floor(n / 16777216) % 256
    b = math.floor(n / 65536) % 256
    g = math.floor(n / 256) % 256
    r = n % 256
    if #v <= 6 then a = 0 end
    return r, g, b, a
  end
  local d = tonumber(s)
  if d then
    if d < 0 then d = d + 4294967296 end
    return d % 256, math.floor(d / 256) % 256, math.floor(d / 65536) % 256, math.floor(d / 16777216) % 256
  end
  error("could not parse color: " .. s)
end

function util.alpha_from_style(c)
  local _, _, _, a = util.extract_color(c)
  return util.ass_alpha(a or 0)
end
function util.color_from_style(c)
  local r, g, b = util.extract_color(c)
  return util.ass_color(r, g, b)
end

function util.interpolate(t, a, b) return a + (b - a) * t end

function util.interpolate_color(t, c1, c2)
  local r1, g1, b1 = util.extract_color(c1)
  local r2, g2, b2 = util.extract_color(c2)
  return util.ass_color(util.interpolate(t, r1, r2), util.interpolate(t, g1, g2), util.interpolate(t, b1, b2))
end

function util.interpolate_alpha(t, a1, a2)
  local x = util.extract_color(a1)
  local y = util.extract_color(a2)
  return util.ass_alpha(util.interpolate(t, x, y))
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
  return h, mx == 0 and 0 or d / mx * 255, mx * 255
end

function util.HSV_to_RGB(h, s, v)
  h = h % 360
  s, v = s / 255, v / 255
  local c = v * s
  local x = c * (1 - math.abs((h / 60) % 2 - 1))
  local m = v - c
  local r, g, b
  if h < 60 then r, g, b = c, x, 0
  elseif h < 120 then r, g, b = x, c, 0
  elseif h < 180 then r, g, b = 0, c, x
  elseif h < 240 then r, g, b = 0, x, c
  elseif h < 300 then r, g, b = x, 0, c
  else r, g, b = c, 0, x end
  return clamp255((r + m) * 255), clamp255((g + m) * 255), clamp255((b + m) * 255)
end

function util.RGB_to_HSL(r, g, b)
  r, g, b = r / 255, g / 255, b / 255
  local mx, mn = math.max(r, g, b), math.min(r, g, b)
  local l = (mx + mn) / 2
  local d = mx - mn
  if d == 0 then return 0, 0, l * 255 end
  local s = l > 0.5 and d / (2 - mx - mn) or d / (mx + mn)
  local h
  if mx == r then h = ((g - b) / d) % 6
  elseif mx == g then h = (b - r) / d + 2
  else h = (r - g) / d + 4 end
  return h * 60, s * 255, l * 255
end

function util.HSL_to_RGB(h, s, l)
  h = h % 360
  s, l = s / 255, l / 255
  local c = (1 - math.abs(2 * l - 1)) * s
  local x = c * (1 - math.abs((h / 60) % 2 - 1))
  local m = l - c / 2
  local r, g, b
  if h < 60 then r, g, b = c, x, 0
  elseif h < 120 then r, g, b = x, c, 0
  elseif h < 180 then r, g, b = 0, c, x
  elseif h < 240 then r, g, b = 0, x, c
  elseif h < 300 then r, g, b = x, 0, c
  else r, g, b = c, 0, x end
  return clamp255((r + m) * 255), clamp255((g + m) * 255), clamp255((b + m) * 255)
end

-- string helpers of Aegisub's util module
function util.trim(s) return (s:gsub("^%s*(.-)%s*$", "%1")) end

-- the first word of a string and the rest
function util.headtail(s)
  local a, b, head, tail = s:find("(.-)%s+(.*)")
  if a then return head, tail end
  return s, ""
end

-- iterate over the words of a string
function util.words(s)
  return function()
    if s == "" then return nil end
    local head, tail = util.headtail(s)
    s = tail
    return head
  end
end

function util.uuid()
  return (("xxxxxxxx-xxxx-4xxx-yxxx-xxxxxxxxxxxx"):gsub("[xy]", function(c)
    local v = c == "x" and math.random(0, 15) or math.random(8, 11)
    return string.format("%x", v)
  end))
end

return util
