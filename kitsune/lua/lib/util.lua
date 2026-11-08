-- Aegisub-compatible `util` module (Kitsune)
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

function util.ass_color(r, g, b)
  return string.format("&H%02X%02X%02X&", b, g, r)
end

function util.ass_alpha(a)
  return string.format("&H%02X&", a)
end

function util.ass_style_color(r, g, b, a)
  return string.format("&H%02X%02X%02X%02X", a or 0, b, g, r)
end

function util.extract_color(s)
  local a, b, g, r
  s = tostring(s)
  -- style color: &HAABBGGRR
  a, b, g, r = s:match("&H(%x%x)(%x%x)(%x%x)(%x%x)")
  if a then return tonumber(r, 16), tonumber(g, 16), tonumber(b, 16), tonumber(a, 16) end
  b, g, r = s:match("&H(%x%x)(%x%x)(%x%x)")
  if b then return tonumber(r, 16), tonumber(g, 16), tonumber(b, 16), 0 end
  local n = s:match("&H(%x+)&?")
  if n then n = tonumber(n, 16); return n % 256, math.floor(n / 256) % 256, math.floor(n / 65536) % 256, 0 end
  local d = tonumber(s)
  if d then return d % 256, math.floor(d / 256) % 256, math.floor(d / 65536) % 256, 0 end
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

function util.interpolate(pct, a, b)
  if pct <= 0 then return a elseif pct >= 1 then return b end
  return a + (b - a) * pct
end

function util.interpolate_color(pct, c1, c2)
  local r1, g1, b1 = util.extract_color(c1)
  local r2, g2, b2 = util.extract_color(c2)
  local f = function(a, b) return math.floor(util.interpolate(pct, a, b) + 0.5) end
  return util.ass_color(f(r1, r2), f(g1, g2), f(b1, b2))
end

function util.interpolate_alpha(pct, a1, a2)
  local x = tonumber((tostring(a1):match("%x+")), 16) or 0
  local y = tonumber((tostring(a2):match("%x+")), 16) or 0
  return util.ass_alpha(math.floor(util.interpolate(pct, x, y) + 0.5))
end

function util.HSV_to_RGB(H, S, V)
  H = H % 360
  local c = V * S
  local x = c * (1 - math.abs((H / 60) % 2 - 1))
  local m = V - c
  local r, g, b
  if H < 60 then r, g, b = c, x, 0
  elseif H < 120 then r, g, b = x, c, 0
  elseif H < 180 then r, g, b = 0, c, x
  elseif H < 240 then r, g, b = 0, x, c
  elseif H < 300 then r, g, b = x, 0, c
  else r, g, b = c, 0, x end
  return math.floor((r + m) * 255 + 0.5), math.floor((g + m) * 255 + 0.5), math.floor((b + m) * 255 + 0.5)
end

function util.HSL_to_RGB(H, S, L)
  H = H % 360
  local c = (1 - math.abs(2 * L - 1)) * S
  local x = c * (1 - math.abs((H / 60) % 2 - 1))
  local m = L - c / 2
  local r, g, b
  if H < 60 then r, g, b = c, x, 0
  elseif H < 120 then r, g, b = x, c, 0
  elseif H < 180 then r, g, b = 0, c, x
  elseif H < 240 then r, g, b = 0, x, c
  elseif H < 300 then r, g, b = x, 0, c
  else r, g, b = c, 0, x end
  return math.floor((r + m) * 255 + 0.5), math.floor((g + m) * 255 + 0.5), math.floor((b + m) * 255 + 0.5)
end

function util.RGB_to_HSV(r, g, b)
  r, g, b = r / 255, g / 255, b / 255
  local mx, mn = math.max(r, g, b), math.min(r, g, b)
  local d = mx - mn
  local h = 0
  if d ~= 0 then
    if mx == r then h = 60 * (((g - b) / d) % 6)
    elseif mx == g then h = 60 * ((b - r) / d + 2)
    else h = 60 * ((r - g) / d + 4) end
  end
  return h, mx == 0 and 0 or d / mx, mx
end

function util.fix_fontname(name)
  return (name:gsub("^@", ""))
end

function util.clamp(v, lo, hi) return math.max(lo, math.min(hi, v)) end

function util.trim(s) return (s:gsub("^%s+", ""):gsub("%s+$", "")) end

function util.string_split(s, sep)
  local r = {}
  for piece in (s .. sep):gmatch("(.-)" .. sep:gsub("%p", "%%%0")) do r[#r + 1] = piece end
  return r
end

return util
