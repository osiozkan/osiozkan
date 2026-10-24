-- Aegisub 3.x 'aegisub.util' module
require("utils")
local u = {}
u.copy = table.copy; u.deep_copy = table.copy_deep
u.clamp = clamp; u.interpolate = interpolate
u.interpolate_color = interpolate_color; u.interpolate_alpha = interpolate_alpha
u.ass_color = ass_color; u.ass_alpha = ass_alpha; u.ass_style_color = ass_style_color
u.extract_color = extract_color; u.alpha_from_style = alpha_from_style
u.color_from_style = color_from_style; u.html_color = html_color
u.trim = string.trim; u.headtail = string.headtail; u.words = string.words
function u.uuid() return "00000000-0000-4000-8000-000000000000" end
function u.tag_from_style(style, tag)
  return nil
end
return u
