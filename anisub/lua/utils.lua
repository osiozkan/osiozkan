-- utils.lua (Aegisub-compatible subset)
function math.round(n, d) d = d or 0; local m = 10 ^ d; return math.floor(n * m + 0.5) / m end
function string.trim(s) return (s:gsub("^%s*(.-)%s*$", "%1")) end
function string.headtail(s)
  local a, b = s:find("%s+")
  if a then return s:sub(1, a - 1), s:sub(b + 1) end
  return s, ""
end
function string.words(s) local r = {}; for w in s:gmatch("%S+") do r[#r + 1] = w end; return r end
function string.starts_with(s, p) return s:sub(1, #p) == p end
function string.ends_with(s, p) return p == "" or s:sub(-#p) == p end
function table.copy(t)
  local r = {}
  for k, v in pairs(t) do r[k] = v end
  return r
end
function table.copy_deep(t)
  if type(t) ~= "table" then return t end
  local r = {}
  for k, v in pairs(t) do r[k] = table.copy_deep(v) end
  return setmetatable(r, getmetatable(t))
end
function table.append(t, ...) for _, v in ipairs({...}) do t[#t + 1] = v end return t end
function table.values(t) local r = {}; for _, v in pairs(t) do r[#r + 1] = v end return r end
function table.keys(t) local r = {}; for k in pairs(t) do r[#r + 1] = k end return r end
function table.contains(t, x) for _, v in pairs(t) do if v == x then return true end end return false end

util = util or {}
function util.copy(t) return table.copy(t) end
function util.deep_copy(t) return table.copy_deep(t) end
function util.ass_color(r, g, b) return string.format("&H%02X%02X%02X&", b, g, r) end
function util.ass_style_color(r, g, b, a) return string.format("&H%02X%02X%02X%02X", a or 0, b, g, r) end
function util.extract_color(s)
  local a, b, g, r = 0, 0, 0, 0
  local hex = s:match("&[Hh](%x+)&?")
  if hex then
    hex = ("0"):rep(8 - #hex) .. hex
    a, b, g, r = tonumber(hex:sub(1, 2), 16), tonumber(hex:sub(3, 4), 16), tonumber(hex:sub(5, 6), 16), tonumber(hex:sub(7, 8), 16)
    return r, g, b, a
  end
  return 0, 0, 0, 0
end
function util.alpha_from_style(c) return string.format("&H%s&", c:sub(3, 4)) end
function util.color_from_style(c) return string.format("&H%s&", c:sub(5, 10)) end
function util.interpolate(t, a, b) return a + (b - a) * t end
function util.clamp(v, lo, hi) return math.max(lo, math.min(hi, v)) end
function util.interpolate_color(t, c1, c2)
  local r1, g1, b1 = util.extract_color(c1)
  local r2, g2, b2 = util.extract_color(c2)
  local f = function(a, b) return math.floor(a + (b - a) * t + 0.5) end
  return util.ass_color(f(r1, r2), f(g1, g2), f(b1, b2))
end
function util.interpolate_alpha(t, a1, a2)
  local x = tonumber(a1:match("%x%x"), 16); local y = tonumber(a2:match("%x%x"), 16)
  return string.format("&H%02X&", math.floor(x + (y - x) * t + 0.5))
end
function util.uuid() return string.format("%08x-%04x-%04x", math.random(0, 0x7fffffff), math.random(0, 0xffff), math.random(0, 0xffff)) end
