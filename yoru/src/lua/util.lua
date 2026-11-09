-- aegisub.util / utils.lua uyumlu yardımcılar
local util = {}

function util.copy(t) local r = {}; for k, v in pairs(t) do r[k] = v end; return r end
function util.deep_copy(t, seen)
  if type(t) ~= "table" then return t end
  seen = seen or {}
  if seen[t] then return seen[t] end
  local r = {}; seen[t] = r
  for k, v in pairs(t) do r[util.deep_copy(k, seen)] = util.deep_copy(v, seen) end
  return setmetatable(r, getmetatable(t))
end
function util.clamp(v, lo, hi) if v < lo then return lo elseif v > hi then return hi end; return v end
local function round(x) return math.floor(x + 0.5) end
function util.interpolate(t, a, b) return a + (b - a) * t end
function util.ass_color(r, g, b) return string.format("&H%02X%02X%02X&", util.clamp(round(b), 0, 255), util.clamp(round(g), 0, 255), util.clamp(round(r), 0, 255)) end
function util.ass_alpha(a) return string.format("&H%02X&", util.clamp(round(a), 0, 255)) end
function util.ass_style_color(r, g, b, a)
  return string.format("&H%02X%02X%02X%02X", util.clamp(round(a or 0), 0, 255), util.clamp(round(b), 0, 255), util.clamp(round(g), 0, 255), util.clamp(round(r), 0, 255))
end
function util.extract_color(s)
  local a, b, g, r
  s = tostring(s)
  -- stil biçimi &HAABBGGRR
  a, b, g, r = s:match("&H(%x%x)(%x%x)(%x%x)(%x%x)")
  if a then return tonumber(r, 16), tonumber(g, 16), tonumber(b, 16), tonumber(a, 16) end
  b, g, r = s:match("&H(%x%x)(%x%x)(%x%x)&?")
  if b then return tonumber(r, 16), tonumber(g, 16), tonumber(b, 16), 0 end
  a = s:match("&H(%x%x)&")
  if a then return 0, 0, 0, tonumber(a, 16) end
  local n = tonumber(s:match("(%d+)"))
  if n then return n % 256, (n // 256) % 256, (n // 65536) % 256, 0 end
  return 0, 0, 0, 0
end
function util.alpha_from_style(s) local _, _, _, a = util.extract_color(s); return util.ass_alpha(a) end
function util.color_from_style(s) local r, g, b = util.extract_color(s); return util.ass_color(r, g, b) end
function util.interpolate_color(t, c1, c2)
  local r1, g1, b1 = util.extract_color(c1); local r2, g2, b2 = util.extract_color(c2)
  return util.ass_color(util.interpolate(t, r1, r2), util.interpolate(t, g1, g2), util.interpolate(t, b1, b2))
end
function util.interpolate_alpha(t, a1, a2)
  local _, _, _, x = util.extract_color(a1); local _, _, _, y = util.extract_color(a2)
  return util.ass_alpha(util.interpolate(t, x, y))
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
  return round(h), round((mx == 0 and 0 or d / mx) * 255), round(mx * 255)
end
function util.HSV_to_RGB(h, s, v)
  s, v = s / 255, v / 255
  local c = v * s; h = (h % 360) / 60
  local x = c * (1 - math.abs(h % 2 - 1)); local m = v - c
  local r, g, b = 0, 0, 0
  if h < 1 then r, g, b = c, x, 0 elseif h < 2 then r, g, b = x, c, 0 elseif h < 3 then r, g, b = 0, c, x
  elseif h < 4 then r, g, b = 0, x, c elseif h < 5 then r, g, b = x, 0, c else r, g, b = c, 0, x end
  return round((r + m) * 255), round((g + m) * 255), round((b + m) * 255)
end
function util.RGB_to_HSL(r, g, b)
  r, g, b = r / 255, g / 255, b / 255
  local mx, mn = math.max(r, g, b), math.min(r, g, b)
  local l = (mx + mn) / 2; local d = mx - mn
  if d == 0 then return 0, 0, round(l * 255) end
  local s = l > 0.5 and d / (2 - mx - mn) or d / (mx + mn)
  local h
  if mx == r then h = (g - b) / d + (g < b and 6 or 0) elseif mx == g then h = (b - r) / d + 2 else h = (r - g) / d + 4 end
  return round(h * 60), round(s * 255), round(l * 255)
end
function util.HSL_to_RGB(h, s, l)
  s, l = s / 255, l / 255; h = (h % 360) / 360
  if s == 0 then local v = round(l * 255); return v, v, v end
  local q = l < 0.5 and l * (1 + s) or l + s - l * s; local p = 2 * l - q
  local function f(t) if t < 0 then t = t + 1 elseif t > 1 then t = t - 1 end
    if t < 1/6 then return p + (q - p) * 6 * t elseif t < 1/2 then return q elseif t < 2/3 then return p + (q - p) * (2/3 - t) * 6 end; return p end
  return round(f(h + 1/3) * 255), round(f(h) * 255), round(f(h - 1/3) * 255)
end
function util.fix_fnt(tag) return tag end
function util.headtail(s) local a, b = s:match("^(.-)%s+(.*)$"); if a then return a, b end; return s, "" end
function util.trim(s) return (s:gsub("^%s+", ""):gsub("%s+$", "")) end
function util.uuid() return string.format("%08x-%04x-%04x", math.random(0, 0xffffffff), math.random(0, 0xffff), math.random(0, 0xffff)) end
function util.tag_value(t) return t end

-- utils.lua genişletmeleri (global tablolar)
function table.copy(t) return util.copy(t) end
function table.copy_deep(t) return util.deep_copy(t) end
function table.append(a, b) for _, v in ipairs(b) do a[#a + 1] = v end; return a end
function table.indexof(t, v) for i, x in ipairs(t) do if x == v then return i end end end
function table.contains(t, v) return table.indexof(t, v) ~= nil end
function string.trim(s) return util.trim(s) end
function string.headtail(s) return util.headtail(s) end
function string.startswith(s, p) return s:sub(1, #p) == p end
function string.endswith(s, p) return p == "" or s:sub(-#p) == p end
function math.round(x, n) n = n or 0; local m = 10 ^ n; return math.floor(x * m + 0.5) / m end
function math.bound(x, lo, hi) return util.clamp(x, lo, hi) end
function math.sign(x) return x > 0 and 1 or (x < 0 and -1 or 0) end

pcall(function() rawset(_G, "util", util) end)
return util
