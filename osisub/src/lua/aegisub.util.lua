-- Aegisub util.lua compatible helpers
local util = {}

function util.copy(t) local r = {}; for k, v in pairs(t) do r[k] = v end; return r end
function util.deep_copy(t)
  local seen = {}
  local function c(x)
    if type(x) ~= 'table' then return x end
    if seen[x] then return seen[x] end
    local r = {}; seen[x] = r
    for k, v in pairs(x) do r[c(k)] = c(v) end
    return setmetatable(r, getmetatable(x))
  end
  return c(t)
end
function util.clamp(v, lo, hi) return math.max(lo, math.min(hi, v)) end
function util.uppercase(s) return s:upper() end
function util.lowercase(s) return s:lower() end
function util.trim(s) local r = s:gsub('^%s+', ''):gsub('%s+$', ''); return r end
function util.ass_color(r, g, b) return string.format('&H%02X%02X%02X&', util.clamp(math.floor(b + .5), 0, 255), util.clamp(math.floor(g + .5), 0, 255), util.clamp(math.floor(r + .5), 0, 255)) end
function util.ass_alpha(a) return string.format('&H%02X&', util.clamp(math.floor(a + .5), 0, 255)) end
function util.ass_style_color(r, g, b, a) return string.format('&H%02X%02X%02X%02X', a or 0, b, g, r) end
function util.extract_color(s)
  local hex = s:match('&[Hh](%x+)')
  if hex then
    local v = tonumber(hex, 16) or 0
    return v & 255, (v >> 8) & 255, (v >> 16) & 255, (v >> 24) & 255
  end
  local r, g, b = s:match('#(%x%x)(%x%x)(%x%x)')
  if r then return tonumber(r, 16), tonumber(g, 16), tonumber(b, 16), 0 end
  return 0, 0, 0, 0
end
function util.alpha_from_style(s) local _, _, _, a = util.extract_color(s); return util.ass_alpha(a) end
function util.color_from_style(s) local r, g, b = util.extract_color(s); return util.ass_color(r, g, b) end
function util.interpolate(t, a, b) return a + (b - a) * t end
function util.interpolate_color(t, c1, c2)
  local r1, g1, b1 = util.extract_color(c1); local r2, g2, b2 = util.extract_color(c2)
  return util.ass_color(util.interpolate(t, r1, r2), util.interpolate(t, g1, g2), util.interpolate(t, b1, b2))
end
function util.interpolate_alpha(t, a1, a2)
  local x = tonumber(a1:match('%x+'), 16) or 0; local y = tonumber(a2:match('%x+'), 16) or 0
  return util.ass_alpha(util.interpolate(t, x, y))
end
function util.RGB_to_HSV(r, g, b)
  r, g, b = r / 255, g / 255, b / 255
  local mx, mn = math.max(r, g, b), math.min(r, g, b); local d = mx - mn
  local h = 0
  if d ~= 0 then
    if mx == r then h = ((g - b) / d) % 6 elseif mx == g then h = (b - r) / d + 2 else h = (r - g) / d + 4 end
    h = h * 60
  end
  return h, mx == 0 and 0 or d / mx, mx
end
function util.HSV_to_RGB(h, s, v)
  h = h % 360; local c = v * s; local x = c * (1 - math.abs((h / 60) % 2 - 1)); local m = v - c
  local r, g, b = 0, 0, 0
  if h < 60 then r, g, b = c, x, 0 elseif h < 120 then r, g, b = x, c, 0 elseif h < 180 then r, g, b = 0, c, x
  elseif h < 240 then r, g, b = 0, x, c elseif h < 300 then r, g, b = x, 0, c else r, g, b = c, 0, x end
  return math.floor((r + m) * 255 + .5), math.floor((g + m) * 255 + .5), math.floor((b + m) * 255 + .5)
end
function util.HSL_to_RGB(h, s, l)
  local c = (1 - math.abs(2 * l - 1)) * s
  local x = c * (1 - math.abs((h % 360 / 60) % 2 - 1)); local m = l - c / 2
  local r, g, b = 0, 0, 0; h = h % 360
  if h < 60 then r, g, b = c, x, 0 elseif h < 120 then r, g, b = x, c, 0 elseif h < 180 then r, g, b = 0, c, x
  elseif h < 240 then r, g, b = 0, x, c elseif h < 300 then r, g, b = x, 0, c else r, g, b = c, 0, x end
  return math.floor((r + m) * 255 + .5), math.floor((g + m) * 255 + .5), math.floor((b + m) * 255 + .5)
end
function util.ass_color_from_hsv(h, s, v) return util.ass_color(util.HSV_to_RGB(h, s, v)) end
function util.fix_fnt(tag) return tag end
function util.headtail(s, sep)
  local a, b = s:find(sep or '%s+')
  if not a then return s, '' end
  return s:sub(1, a - 1), s:sub(b + 1)
end
function util.string_split(s, sep) local r = {}; for p in (s .. sep):gmatch('(.-)' .. sep:gsub('%p', '%%%0')) do r[#r + 1] = p end; return r end
function util.words(s) return s:gmatch('%S+') end
function util.string_split(s, sep) local r = {}; for p in (s .. sep):gmatch('(.-)' .. sep:gsub('%p', '%%%0')) do r[#r + 1] = p end; return r end
function util.print_table(t, indent)
  indent = indent or ''
  for k, v in pairs(t) do
    if type(v) == 'table' then aegisub.log('%s%s:\n', indent, tostring(k)); util.print_table(v, indent .. '  ')
    else aegisub.log('%s%s: %s\n', indent, tostring(k), tostring(v)) end
  end
end
function util.time_to_str(ms) return string.format('%d:%02d:%02d.%03d', ms // 3600000, ms // 60000 % 60, ms // 1000 % 60, ms % 1000) end
function util.frame_to_time(f) return aegisub.ms_from_frame(f) end
function util.assderror(msg) error(msg, 2) end
function util.tag_is_string() return false end
util.words = util.words
return util
