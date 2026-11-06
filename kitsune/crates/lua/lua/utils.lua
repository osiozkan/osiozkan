-- Eski stil `include("utils.lua")`: genel fonksiyonlar tanımlar.
local util = require("aegisub.util")
table.copy = util.copy
table.copy_deep = util.deep_copy
string.trim = util.trim
string.headtail = util.headtail
string.words = util.words
function math.round(x, n) local m = 10 ^ (n or 0); return math.floor(x * m + 0.5) / m end
function clamp(v, lo, hi) return util.clamp(v, lo, hi) end
function interpolate(p, a, b) return util.interpolate(p, a, b) end
ass_color, ass_alpha, ass_style_color = util.ass_color, util.ass_alpha, util.ass_style_color
extract_color, alpha_from_style, color_from_style = util.extract_color, util.alpha_from_style, util.color_from_style
interpolate_color, interpolate_alpha = util.interpolate_color, util.interpolate_alpha
HSV_to_RGB, HSL_to_RGB, RGB_to_HSV = util.HSV_to_RGB, util.HSL_to_RGB, util.RGB_to_HSV
