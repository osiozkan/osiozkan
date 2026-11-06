-- aegisub.util uyumlu yardımcılar (Aegisub 3.x utils.lua / aegisub.util)
local util = {}

function util.copy(t)
  local r = {}
  for k, v in pairs(t) do r[k] = v end
  return r
end
function util.deep_copy(t)
  local seen = {}
  local function c(o)
    if type(o) ~= "table" then return o end
    if seen[o] then return seen[o] end
    local r = {}
    seen[o] = r
    for k, v in pairs(o) do r[c(k)] = c(v) end
    return setmetatable(r, getmetatable(o))
  end
  return c(t)
end
function util.clamp(v, lo, hi) if v < lo then return lo elseif v > hi then return hi end return v end
function util.interpolate(pct, a, b) return a + (b - a) * pct end

function util.ass_color(r, g, b) return string.format("&H%02X%02X%02X&", b, g, r) end
function util.ass_alpha(a) return string.format("&H%02X&", a) end
function util.ass_style_color(r, g, b, a) return string.format("&H%02X%02X%02X%02X", a or 0, b, g, r) end

function util.extract_color(s)
  local a, b, g, r
  s = tostring(s or "")
  -- &HAABBGGRR  (stil rengi)
  a, b, g, r = s:match("&H(%x%x)(%x%x)(%x%x)(%x%x)")
  if a then return tonumber(r, 16), tonumber(g, 16), tonumber(b, 16), tonumber(a, 16) end
  -- &HBBGGRR&  (etiket rengi)
  b, g, r = s:match("&H(%x%x)(%x%x)(%x%x)&?")
  if b then return tonumber(r, 16), tonumber(g, 16), tonumber(b, 16), 0 end
  -- &HAA&
  a = s:match("&H(%x%x)&")
  if a then return 0, 0, 0, tonumber(a, 16) end
  return 0, 0, 0, 0
end
function util.alpha_from_style(s) local _, _, _, a = util.extract_color(s); return util.ass_alpha(a) end
function util.color_from_style(s) local r, g, b = util.extract_color(s); return util.ass_color(r, g, b) end

function util.interpolate_color(pct, c1, c2)
  local r1, g1, b1 = util.extract_color(c1)
  local r2, g2, b2 = util.extract_color(c2)
  return util.ass_color(math.floor(util.interpolate(pct, r1, r2) + 0.5),
                        math.floor(util.interpolate(pct, g1, g2) + 0.5),
                        math.floor(util.interpolate(pct, b1, b2) + 0.5))
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
  local s = mx == 0 and 0 or d / mx
  return math.floor(h + 0.5), math.floor(s * 255 + 0.5), math.floor(mx * 255 + 0.5)
end
function util.HSV_to_RGB(h, s, v)
  s, v = s / 255, v / 255
  h = (h % 360) / 60
  local c = v * s
  local x = c * (1 - math.abs(h % 2 - 1))
  local r, g, b = 0, 0, 0
  if h < 1 then r, g, b = c, x, 0 elseif h < 2 then r, g, b = x, c, 0
  elseif h < 3 then r, g, b = 0, c, x elseif h < 4 then r, g, b = 0, x, c
  elseif h < 5 then r, g, b = x, 0, c else r, g, b = c, 0, x end
  local m = v - c
  return math.floor((r + m) * 255 + 0.5), math.floor((g + m) * 255 + 0.5), math.floor((b + m) * 255 + 0.5)
end
function util.HSL_to_RGB(h, s, l)
  s, l = s / 255, l / 255
  local c = (1 - math.abs(2 * l - 1)) * s
  local hp = (h % 360) / 60
  local x = c * (1 - math.abs(hp % 2 - 1))
  local r, g, b = 0, 0, 0
  if hp < 1 then r, g, b = c, x, 0 elseif hp < 2 then r, g, b = x, c, 0
  elseif hp < 3 then r, g, b = 0, c, x elseif hp < 4 then r, g, b = 0, x, c
  elseif hp < 5 then r, g, b = x, 0, c else r, g, b = c, 0, x end
  local m = l - c / 2
  return math.floor((r + m) * 255 + 0.5), math.floor((g + m) * 255 + 0.5), math.floor((b + m) * 255 + 0.5)
end

function util.trim(s) return (s:gsub("^%s+", ""):gsub("%s+$", "")) end
function util.headtail(s)
  local a, b = s:find("%s+")
  if a then return s:sub(1, a - 1), s:sub(b + 1) end
  return s, ""
end
function util.words(s)
  return function()
    if s == "" then return nil end
    local a, b = s:find("%s+")
    local w, sp
    if a == 1 then w, sp, s = "", s:sub(1, b), s:sub(b + 1)
    elseif a then w, sp, s = s:sub(1, a - 1), s:sub(a, b), s:sub(b + 1)
    else w, sp, s = s, "", "" end
    return w, sp
  end
end
function util.uuid()
  return (("xxxxxxxx-xxxx-4xxx-yxxx-xxxxxxxxxxxx"):gsub("[xy]", function(c)
    local v = c == "x" and math.random(0, 15) or math.random(8, 11)
    return string.format("%x", v)
  end))
end

return util
