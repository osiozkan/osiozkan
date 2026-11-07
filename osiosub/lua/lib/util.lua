-- OsioSub util.lua : compatible with Aegisub's util.lua
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

function util.clamp(v, lo, hi) return math.max(lo, math.min(hi, v)) end
function util.min(a, b) return a < b and a or b end
function util.max(a, b) return a > b and a or b end

function util.ass_color(r, g, b)
  return string.format("&H%02X%02X%02X&", util.clamp(math.floor(b + 0.5), 0, 255), util.clamp(math.floor(g + 0.5), 0, 255), util.clamp(math.floor(r + 0.5), 0, 255))
end
function util.ass_alpha(a) return string.format("&H%02X&", util.clamp(math.floor(a + 0.5), 0, 255)) end
function util.ass_style_color(r, g, b, a)
  return string.format("&H%02X%02X%02X%02X", util.clamp(math.floor((a or 0) + 0.5), 0, 255), util.clamp(math.floor(b + 0.5), 0, 255), util.clamp(math.floor(g + 0.5), 0, 255), util.clamp(math.floor(r + 0.5), 0, 255))
end

function util.extract_color(s)
  local a, b, g, r = 0, 0, 0, 0
  local alpha = s:match("^&H(%x%x)&$")
  if alpha then return tonumber(alpha, 16), 0, 0, 0 end
  local hex = s:match("^&H(%x+)&?$")
  if hex then
    hex = string.rep("0", 8 - #hex) .. hex
    a, b, g, r = hex:match("(%x%x)(%x%x)(%x%x)(%x%x)")
    return tonumber(r, 16), tonumber(g, 16), tonumber(b, 16), tonumber(a, 16)
  end
  local n = s:match("^(%d+)$")
  if n then
    n = tonumber(n)
    return n % 256, math.floor(n / 256) % 256, math.floor(n / 65536) % 256, 0
  end
  return 0, 0, 0, 0
end
function util.alpha_from_style(s) local _, _, _, a = util.extract_color(s); return util.ass_alpha(a) end
function util.color_from_style(s) local r, g, b = util.extract_color(s); return util.ass_color(r, g, b) end

function util.HSV_to_RGB(H, S, V)
  H = H % 360
  local c = V * S / 255
  local h = H / 60
  local x = c * (1 - math.abs(h % 2 - 1))
  local r, g, b = 0, 0, 0
  if h < 1 then r, g, b = c, x, 0 elseif h < 2 then r, g, b = x, c, 0 elseif h < 3 then r, g, b = 0, c, x
  elseif h < 4 then r, g, b = 0, x, c elseif h < 5 then r, g, b = x, 0, c else r, g, b = c, 0, x end
  local m = V - c
  return math.floor(r + m + 0.5), math.floor(g + m + 0.5), math.floor(b + m + 0.5)
end
function util.HSL_to_RGB(H, S, L)
  H = H % 360
  local s, l = S / 255, L / 255
  local c = (1 - math.abs(2 * l - 1)) * s
  local h = H / 60
  local x = c * (1 - math.abs(h % 2 - 1))
  local r, g, b = 0, 0, 0
  if h < 1 then r, g, b = c, x, 0 elseif h < 2 then r, g, b = x, c, 0 elseif h < 3 then r, g, b = 0, c, x
  elseif h < 4 then r, g, b = 0, x, c elseif h < 5 then r, g, b = x, 0, c else r, g, b = c, 0, x end
  local m = l - c / 2
  return math.floor((r + m) * 255 + 0.5), math.floor((g + m) * 255 + 0.5), math.floor((b + m) * 255 + 0.5)
end
function util.RGB_to_HSV(r, g, b)
  local mx, mn = math.max(r, g, b), math.min(r, g, b)
  local d = mx - mn
  local h = 0
  if d ~= 0 then
    if mx == r then h = ((g - b) / d) % 6 elseif mx == g then h = (b - r) / d + 2 else h = (r - g) / d + 4 end
    h = h * 60
  end
  local s = mx == 0 and 0 or d / mx * 255
  return math.floor(h + 0.5), math.floor(s + 0.5), mx
end
function util.RGB_to_HSL(r, g, b)
  local mx, mn = math.max(r, g, b), math.min(r, g, b)
  local d = (mx - mn) / 255
  local l = (mx + mn) / 510
  local h = 0
  if d ~= 0 then
    local dd = mx - mn
    if mx == r then h = ((g - b) / dd) % 6 elseif mx == g then h = (b - r) / dd + 2 else h = (r - g) / dd + 4 end
    h = h * 60
  end
  local s = d == 0 and 0 or d / (1 - math.abs(2 * l - 1))
  return math.floor(h + 0.5), math.floor(s * 255 + 0.5), math.floor(l * 255 + 0.5)
end

function util.interpolate(pct, a, b) return a + (b - a) * pct end
function util.interpolate_color(pct, c1, c2)
  local r1, g1, b1 = util.extract_color(c1)
  local r2, g2, b2 = util.extract_color(c2)
  return util.ass_color(util.interpolate(pct, r1, r2), util.interpolate(pct, g1, g2), util.interpolate(pct, b1, b2))
end
function util.interpolate_alpha(pct, a1, a2)
  local _, _, _, x = util.extract_color(a1)
  local _, _, _, y = util.extract_color(a2)
  return util.ass_alpha(util.interpolate(pct, x, y))
end

function util.fix_fontname(name)
  if name:sub(1, 1) == "@" then return name:sub(2) end
  return name
end

function util.uuid()
  local t = {}
  for i = 1, 16 do t[i] = math.random(0, 255) end
  t[7] = t[7] % 16 + 64; t[9] = t[9] % 64 + 128
  return string.format("%02x%02x%02x%02x-%02x%02x-%02x%02x-%02x%02x-%02x%02x%02x%02x%02x%02x", unpack(t))
end

function util.trim(s) return (s:gsub("^%s+", ""):gsub("%s+$", "")) end
function util.headtail(s)
  local a, b = s:match("^%s*(%S+)%s*(.-)%s*$")
  if not a then return "", "" end
  return a, b
end
function util.words(s)
  return function()
    local w
    w, s = s:match("^%s*(%S+)%s*(.*)$")
    return w
  end
end

function string.trim(s) return util.trim(s) end
function string.headtail(s) return util.headtail(s) end
function string.words(s) return util.words(s) end
function table.copy(t) return util.copy(t) end
function table.deep_copy(t) return util.deep_copy(t) end

return util
