-- Aegisub-compatible utils.lua (subset, API-compatible)
local util = {}
function util.copy(t) local r = {}; for k, v in pairs(t) do r[k] = v end; return r end
function util.deep_copy(t)
  if type(t) ~= "table" then return t end
  local r = {}
  for k, v in pairs(t) do r[util.deep_copy(k)] = util.deep_copy(v) end
  return setmetatable(r, getmetatable(t))
end
function util.clamp(v, lo, hi) return math.max(lo, math.min(hi, v)) end
function util.interpolate(t, a, b) return a + (b - a) * t end
function util.interpolate_color(t, c1, c2)
  local r1, g1, b1 = util.extract_color(c1); local r2, g2, b2 = util.extract_color(c2)
  return util.ass_color(math.floor(util.interpolate(t, r1, r2) + .5), math.floor(util.interpolate(t, g1, g2) + .5), math.floor(util.interpolate(t, b1, b2) + .5))
end
function util.interpolate_alpha(t, a1, a2)
  return util.ass_alpha(math.floor(util.interpolate(t, tonumber(a1:match("%x%x"), 16) or 0, tonumber(a2:match("%x%x"), 16) or 0) + .5))
end
function util.ass_color(r, g, b) return string.format("&H%02X%02X%02X&", b, g, r) end
function util.ass_alpha(a) return string.format("&H%02X&", a) end
function util.ass_style_color(r, g, b, a) return string.format("&H%02X%02X%02X%02X", a or 0, b, g, r) end
function util.extract_color(s)
  local a, b, g, r = nil, nil, nil, nil
  local h = s:match("&?[Hh](%x+)")
  if h then
    h = string.rep("0", 8 - #h) .. h
    a, b, g, r = tonumber(h:sub(1,2),16), tonumber(h:sub(3,4),16), tonumber(h:sub(5,6),16), tonumber(h:sub(7,8),16)
    return r, g, b, a
  end
  local rr, gg, bb = s:match("#(%x%x)(%x%x)(%x%x)")
  if rr then return tonumber(rr,16), tonumber(gg,16), tonumber(bb,16), 0 end
  return 0, 0, 0, 0
end
function util.alpha_from_style(s) local _,_,_,a = util.extract_color(s); return util.ass_alpha(a) end
function util.color_from_style(s) local r,g,b = util.extract_color(s); return util.ass_color(r,g,b) end
function util.HSV_to_RGB(h, s, v)
  h = h % 360; local c = v * s; local x = c * (1 - math.abs((h / 60) % 2 - 1)); local m = v - c
  local r, g, b = 0, 0, 0
  if h < 60 then r,g,b = c,x,0 elseif h < 120 then r,g,b = x,c,0 elseif h < 180 then r,g,b = 0,c,x
  elseif h < 240 then r,g,b = 0,x,c elseif h < 300 then r,g,b = x,0,c else r,g,b = c,0,x end
  return math.floor((r+m)*255+.5), math.floor((g+m)*255+.5), math.floor((b+m)*255+.5)
end
function util.HSL_to_RGB(h, s, l)
  local c = (1 - math.abs(2 * l - 1)) * s
  local hp = (h % 360) / 60; local x = c * (1 - math.abs(hp % 2 - 1)); local m = l - c / 2
  local r, g, b = 0, 0, 0
  if hp < 1 then r,g,b = c,x,0 elseif hp < 2 then r,g,b = x,c,0 elseif hp < 3 then r,g,b = 0,c,x
  elseif hp < 4 then r,g,b = 0,x,c elseif hp < 5 then r,g,b = x,0,c else r,g,b = c,0,x end
  return math.floor((r+m)*255+.5), math.floor((g+m)*255+.5), math.floor((b+m)*255+.5)
end
function util.fade(alpha_in, alpha_out, t1, t2, t3, t4) return string.format("\\fade(%d,%d,%d,%d,%d,%d)", alpha_in, 255, alpha_out, t1, t2, t3) end
function util.trim(s) return (s:gsub("^%s+", ""):gsub("%s+$", "")) end
function util.headtail(s, pat) pat = pat or "%s+"; local a, b = s:find(pat); if a then return s:sub(1, a-1), s:sub(b+1) end; return s, "" end
function util.string_split(s, sep) local r = {}; for p in (s..sep):gmatch("(.-)"..sep:gsub("%p","%%%0")) do r[#r+1] = p end; return r end
function util.words(s) return s:gmatch("%S+") end
function util.uuid() return string.format("%08x-%04x", math.random(0, 0x7fffffff), math.random(0, 0xffff)) end
function util.frames_to_ms(f, fps) return aegisub.ms_from_frame(f) end
function util.is_windows() return false end
return util
