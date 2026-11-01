-- Legacy global helper library (Auto3/Auto4 'utils.lua')
local util = require("aegisub.util")

for _, n in ipairs({"interpolate", "interpolate_color", "interpolate_alpha", "ass_color", "ass_alpha",
                    "ass_style_color", "extract_color", "alpha_from_style", "color_from_style",
                    "HSV_to_RGB", "RGB_to_HSV", "HSL_to_RGB", "RGB_to_HSL"}) do
  _G[n] = util[n]
end

function math.round(n, dec)
  dec = dec or 0
  local m = 10 ^ dec
  return math.floor(n * m + 0.5) / m
end
function math.clamp(v, lo, hi) return util.clamp(v, lo, hi) end
function string.trim(s) return util.trim(s) end
function string.headtail(s) return util.headtail(s) end
function string.words(s) return util.words(s) end
function table.copy(t) return util.copy(t) end
function table.copy_deep(t) return util.deep_copy(t) end
function table.append(a, b) for _, v in ipairs(b) do a[#a+1] = v end return a end
function table.indexof(t, v) for i, x in ipairs(t) do if x == v then return i end end end
function string.startswith(s, p) return s:sub(1, #p) == p end
function string.endswith(s, p) return p == "" or s:sub(-#p) == p end
function string.split(s, sep)
  local r = {}
  for piece in (s .. sep):gmatch("(.-)" .. sep:gsub("%p", "%%%0")) do r[#r+1] = piece end
  return r
end
