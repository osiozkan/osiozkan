-- osisub: Aegisub utils.lua uyumlu yardımcılar

util = {}

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

function util.clamp(v, lo, hi)
  if v < lo then return lo elseif v > hi then return hi end
  return v
end

local function round(x) return math.floor(x + 0.5) end
if not math.round then math.round = round end

function util.ass_color(r, g, b)
  return string.format("&H%02X%02X%02X&", util.clamp(round(b), 0, 255), util.clamp(round(g), 0, 255), util.clamp(round(r), 0, 255))
end

function util.ass_alpha(a)
  return string.format("&H%02X&", util.clamp(round(a), 0, 255))
end

function util.ass_style_color(r, g, b, a)
  return string.format("&H%02X%02X%02X%02X", util.clamp(round(a or 0), 0, 255), util.clamp(round(b), 0, 255), util.clamp(round(g), 0, 255), util.clamp(round(r), 0, 255))
end

-- "&HAABBGGRR", "&HBBGGRR&", "\c&H..&" ve "#RRGGBB" biçimlerini r,g,b,a olarak çözer
function util.extract_color(s)
  local hex = s:match("#(%x%x%x%x%x%x)")
  if hex then
    return tonumber(hex:sub(1, 2), 16), tonumber(hex:sub(3, 4), 16), tonumber(hex:sub(5, 6), 16), 0
  end
  hex = s:match("&?[Hh](%x+)&?")
  if not hex then return 0, 0, 0, 0 end
  hex = string.rep("0", math.max(0, 6 - #hex)) .. hex
  if #hex >= 8 then
    hex = hex:sub(-8)
    return tonumber(hex:sub(7, 8), 16), tonumber(hex:sub(5, 6), 16), tonumber(hex:sub(3, 4), 16), tonumber(hex:sub(1, 2), 16)
  end
  hex = hex:sub(-6)
  return tonumber(hex:sub(5, 6), 16), tonumber(hex:sub(3, 4), 16), tonumber(hex:sub(1, 2), 16), 0
end

function util.alpha_from_style(s)
  local _, _, _, a = util.extract_color(s)
  return util.ass_alpha(a)
end

function util.color_from_style(s)
  local r, g, b = util.extract_color(s)
  return util.ass_color(r, g, b)
end

function util.interpolate(t, a, b)
  return a + (b - a) * t
end

function util.interpolate_color(t, c1, c2)
  local r1, g1, b1 = util.extract_color(c1)
  local r2, g2, b2 = util.extract_color(c2)
  return util.ass_color(util.interpolate(t, r1, r2), util.interpolate(t, g1, g2), util.interpolate(t, b1, b2))
end

function util.interpolate_alpha(t, a1, a2)
  local _, _, _, x1 = util.extract_color(a1)
  local _, _, _, x2 = util.extract_color(a2)
  local function only(s)
    local h = s:match("&?[Hh](%x%x)&?$")
    return h and tonumber(h, 16)
  end
  x1 = (a1:match("^&?[Hh]%x%x&?$") and only(a1)) or x1
  x2 = (a2:match("^&?[Hh]%x%x&?$") and only(a2)) or x2
  return util.ass_alpha(util.interpolate(t, x1, x2))
end

-- H derece [0,360); S,V [0,1] (255 ölçeği de kabul edilir). r,g,b [0,255] döner.
function util.HSV_to_RGB(H, S, V)
  if S > 1 or V > 1 then S, V = S / 255, V / 255 end
  H = (H % 360) / 60
  local c = V * S
  local x = c * (1 - math.abs(H % 2 - 1))
  local m = V - c
  local r, g, b
  local i = math.floor(H)
  if i == 0 then r, g, b = c, x, 0
  elseif i == 1 then r, g, b = x, c, 0
  elseif i == 2 then r, g, b = 0, c, x
  elseif i == 3 then r, g, b = 0, x, c
  elseif i == 4 then r, g, b = x, 0, c
  else r, g, b = c, 0, x end
  return round((r + m) * 255), round((g + m) * 255), round((b + m) * 255)
end

function util.HSL_to_RGB(H, S, L)
  if S > 1 or L > 1 then S, L = S / 255, L / 255 end
  local c = (1 - math.abs(2 * L - 1)) * S
  H = (H % 360) / 60
  local x = c * (1 - math.abs(H % 2 - 1))
  local m = L - c / 2
  local r, g, b
  local i = math.floor(H)
  if i == 0 then r, g, b = c, x, 0
  elseif i == 1 then r, g, b = x, c, 0
  elseif i == 2 then r, g, b = 0, c, x
  elseif i == 3 then r, g, b = 0, x, c
  elseif i == 4 then r, g, b = x, 0, c
  else r, g, b = c, 0, x end
  return round((r + m) * 255), round((g + m) * 255), round((b + m) * 255)
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
  return h * 60, mx == 0 and 0 or d / mx, mx
end

function util.RGB_to_HSL(r, g, b)
  r, g, b = r / 255, g / 255, b / 255
  local mx, mn = math.max(r, g, b), math.min(r, g, b)
  local d = mx - mn
  local l = (mx + mn) / 2
  local h
  if d == 0 then h = 0
  elseif mx == r then h = ((g - b) / d) % 6
  elseif mx == g then h = (b - r) / d + 2
  else h = (r - g) / d + 4 end
  local s = d == 0 and 0 or d / (1 - math.abs(2 * l - 1))
  return h * 60, s, l
end

function util.uuid()
  local t = {}
  for i = 1, 32 do t[i] = string.format("%x", math.random(0, 15)) end
  local s = table.concat(t)
  return s:sub(1, 8) .. "-" .. s:sub(9, 12) .. "-4" .. s:sub(14, 16) .. "-a" .. s:sub(18, 20) .. "-" .. s:sub(21, 32)
end

function string.trim(s)
  return (s:gsub("^%s*(.-)%s*$", "%1"))
end

function string.headtail(s)
  local a, b = s:find("%s+")
  if a then return s:sub(1, a - 1), s:sub(b + 1) end
  return s, ""
end

function string.words(s)
  return s:gmatch("%S+")
end
