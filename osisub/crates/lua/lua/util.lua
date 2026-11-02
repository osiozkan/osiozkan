-- Aegisub aegisub.util compatible helpers
local util = {}

function util.copy(t)
  local r = {}
  for k, v in pairs(t) do r[k] = v end
  return r
end

function util.deep_copy(t)
  local seen = {}
  local function cp(o)
    if type(o) ~= "table" then return o end
    if seen[o] then return seen[o] end
    local r = {}
    seen[o] = r
    for k, v in pairs(o) do r[cp(k)] = cp(v) end
    return setmetatable(r, getmetatable(o))
  end
  return cp(t)
end

local function clamp(v, lo, hi) if v < lo then return lo elseif v > hi then return hi end return v end
util.clamp = clamp

function util.ass_color(r, g, b)
  return string.format("&H%02X%02X%02X&", clamp(b, 0, 255), clamp(g, 0, 255), clamp(r, 0, 255))
end
function util.ass_alpha(a) return string.format("&H%02X&", clamp(a, 0, 255)) end
function util.ass_style_color(r, g, b, a)
  return string.format("&H%02X%02X%02X%02X", clamp(a or 0, 0, 255), clamp(b, 0, 255), clamp(g, 0, 255), clamp(r, 0, 255))
end

-- returns r, g, b, a
function util.extract_color(s)
  local a, b, g, r = s:match("&[Hh](%x%x)(%x%x)(%x%x)(%x%x)")
  if a then return tonumber(r, 16), tonumber(g, 16), tonumber(b, 16), tonumber(a, 16) end
  b, g, r = s:match("&[Hh](%x%x)(%x%x)(%x%x)")
  if b then return tonumber(r, 16), tonumber(g, 16), tonumber(b, 16), 0 end
  a = s:match("&[Hh](%x%x)&?$")
  if a then return 0, 0, 0, tonumber(a, 16) end
  local n = tonumber(s)
  if n then return n % 256, math.floor(n / 256) % 256, math.floor(n / 65536) % 256, 0 end
  return 0, 0, 0, 0
end

function util.alpha_from_style(s) local _, _, _, a = util.extract_color(s); return util.ass_alpha(a) end
function util.color_from_style(s) local r, g, b = util.extract_color(s); return util.ass_color(r, g, b) end

function util.interpolate(pct, min, max)
  if pct <= 0 then return min elseif pct >= 1 then return max end
  return pct * (max - min) + min
end
function util.interpolate_color(pct, first, last)
  local r1, g1, b1 = util.extract_color(first)
  local r2, g2, b2 = util.extract_color(last)
  local f = function(a, b) return math.floor(util.interpolate(pct, a, b) + 0.5) end
  return util.ass_color(f(r1, r2), f(g1, g2), f(b1, b2))
end
function util.interpolate_alpha(pct, first, last)
  local _, _, _, a1 = util.extract_color(first)
  local _, _, _, a2 = util.extract_color(last)
  return util.ass_alpha(math.floor(util.interpolate(pct, a1, a2) + 0.5))
end

-- H: 0..360, S,V,L: 0..1, RGB: 0..255
function util.HSV_to_RGB(H, S, V)
  H = H % 360
  local c = V * S
  local x = c * (1 - math.abs((H / 60) % 2 - 1))
  local m = V - c
  local r, g, b
  if H < 60 then r, g, b = c, x, 0 elseif H < 120 then r, g, b = x, c, 0
  elseif H < 180 then r, g, b = 0, c, x elseif H < 240 then r, g, b = 0, x, c
  elseif H < 300 then r, g, b = x, 0, c else r, g, b = c, 0, x end
  local f = function(v) return math.floor((v + m) * 255 + 0.5) end
  return f(r), f(g), f(b)
end
function util.RGB_to_HSV(r, g, b)
  r, g, b = r / 255, g / 255, b / 255
  local mx, mn = math.max(r, g, b), math.min(r, g, b)
  local d = mx - mn
  local h = 0
  if d ~= 0 then
    if mx == r then h = ((g - b) / d) % 6 elseif mx == g then h = (b - r) / d + 2 else h = (r - g) / d + 4 end
    h = h * 60
  end
  return h, (mx == 0) and 0 or d / mx, mx
end
function util.HSL_to_RGB(H, S, L)
  H = H % 360
  local c = (1 - math.abs(2 * L - 1)) * S
  local x = c * (1 - math.abs((H / 60) % 2 - 1))
  local m = L - c / 2
  local r, g, b
  if H < 60 then r, g, b = c, x, 0 elseif H < 120 then r, g, b = x, c, 0
  elseif H < 180 then r, g, b = 0, c, x elseif H < 240 then r, g, b = 0, x, c
  elseif H < 300 then r, g, b = x, 0, c else r, g, b = c, 0, x end
  local f = function(v) return math.floor((v + m) * 255 + 0.5) end
  return f(r), f(g), f(b)
end
function util.RGB_to_HSL(r, g, b)
  r, g, b = r / 255, g / 255, b / 255
  local mx, mn = math.max(r, g, b), math.min(r, g, b)
  local d = mx - mn
  local l = (mx + mn) / 2
  local h, s = 0, 0
  if d ~= 0 then
    s = d / (1 - math.abs(2 * l - 1))
    if mx == r then h = ((g - b) / d) % 6 elseif mx == g then h = (b - r) / d + 2 else h = (r - g) / d + 4 end
    h = h * 60
  end
  return h, s, l
end

function util.string_trim(s) return (s:gsub("^%s+", ""):gsub("%s+$", "")) end
function util.headtail(s)
  local a, b = s:match("^(%S*)%s+(.*)$")
  if a then return a, b end
  return s, ""
end
function util.words(s)
  return function()
    if s == "" then return nil end
    local w, sp, rest = s:match("^(%S+)(%s*)(.*)$")
    if not w then
      local sp2 = s; s = ""; return "", sp2
    end
    s = rest
    return w, sp
  end
end
function util.fix_fnt(tag) return tag end

return util
