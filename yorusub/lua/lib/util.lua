-- Aegisub util.lua uyumlu
local util = {}
function util.copy(t) local r = {} for k, v in pairs(t) do r[k] = v end return r end
function util.deep_copy(t)
  if type(t) ~= "table" then return t end
  local r = {}
  for k, v in pairs(t) do r[util.deep_copy(k)] = util.deep_copy(v) end
  return setmetatable(r, getmetatable(t))
end
function util.clamp(v, lo, hi) if v < lo then return lo elseif v > hi then return hi end return v end
function util.interpolate(t, a, b) return a + (b - a) * t end
function util.ass_color(r, g, b) return string.format("&H%02X%02X%02X&", b, g, r) end
function util.ass_alpha(a) return string.format("&H%02X&", a) end
function util.ass_style_color(r, g, b, a) return string.format("&H%02X%02X%02X%02X", a or 0, b, g, r) end
function util.extract_color(s)
  local a, b, g, r
  s = tostring(s)
  local m = s:match("&?[Hh](%x+)&?")
  if not m then return 0, 0, 0, 0 end
  m = string.rep("0", 8 - #m) .. m
  a, b, g, r = m:match("(%x%x)(%x%x)(%x%x)(%x%x)")
  return tonumber(r, 16), tonumber(g, 16), tonumber(b, 16), tonumber(a, 16)
end
function util.alpha_from_style(s) local _, _, _, a = util.extract_color(s) return util.ass_alpha(a) end
function util.color_from_style(s) local r, g, b = util.extract_color(s) return util.ass_color(r, g, b) end
function util.interpolate_color(t, c1, c2)
  local r1, g1, b1 = util.extract_color(c1)
  local r2, g2, b2 = util.extract_color(c2)
  local f = function(a, b) return math.floor(util.interpolate(t, a, b) + 0.5) end
  return util.ass_color(f(r1, r2), f(g1, g2), f(b1, b2))
end
function util.interpolate_alpha(t, a1, a2)
  local x = tonumber(a1:match("%x+"), 16); local y = tonumber(a2:match("%x+"), 16)
  return util.ass_alpha(math.floor(util.interpolate(t, x, y) + 0.5))
end
function util.fade(a1, a2, a3, t1, t2, t3, t4, t)
  if t < t1 then return a1 elseif t < t2 then return util.interpolate((t-t1)/(t2-t1), a1, a2)
  elseif t < t3 then return a2 elseif t < t4 then return util.interpolate((t-t3)/(t4-t3), a2, a3) end
  return a3
end
function util.HSV_to_RGB(H, S, V)
  H = H % 360; S = S / 255; V = V / 255
  local c = V * S; local x = c * (1 - math.abs((H / 60) % 2 - 1)); local m = V - c
  local r, g, b
  if H < 60 then r, g, b = c, x, 0 elseif H < 120 then r, g, b = x, c, 0
  elseif H < 180 then r, g, b = 0, c, x elseif H < 240 then r, g, b = 0, x, c
  elseif H < 300 then r, g, b = x, 0, c else r, g, b = c, 0, x end
  return math.floor((r+m)*255+.5), math.floor((g+m)*255+.5), math.floor((b+m)*255+.5)
end
function util.RGB_to_HSV(r, g, b)
  r, g, b = r/255, g/255, b/255
  local mx, mn = math.max(r,g,b), math.min(r,g,b); local d = mx - mn
  local h = 0
  if d > 0 then
    if mx == r then h = 60 * (((g-b)/d) % 6) elseif mx == g then h = 60 * ((b-r)/d + 2) else h = 60 * ((r-g)/d + 4) end
  end
  return h, (mx == 0) and 0 or d/mx*255, mx*255
end
function util.uni_chars(s) local t = {} for c in s:gmatch("[%z\1-\127\194-\244][\128-\191]*") do t[#t+1] = c end return t end
function util.trim(s) return (s:gsub("^%s+", ""):gsub("%s+$", "")) end
function util.headtail(s) return s:headtail() end
function util.words(s) local t = {} for w in s:gmatch("%S+") do t[#t+1] = w end return t end
function util.retime(mode, a, b, sub) return 0 end
function util.tag_strip(s) return (s:gsub("{[^}]*}", "")) end
function util.fix_fnt(s) return s end
util.useful = true
_G.util = util
return util
