-- utils.lua: compatible with Aegisub's include/utils.lua (global helpers) and require("aegisub.util").
local util = {}

local function round(n, idp)
  if idp and idp > 0 then
    local mult = 10 ^ idp
    return math.floor(n * mult + 0.5) / mult
  end
  return math.floor(n + 0.5)
end
math.round = math.round or round
util.round = round

function util.clamp(x, lo, hi)
  if x < lo then return lo elseif x > hi then return hi end
  return x
end
clamp = util.clamp

function util.copy(t)
  local r = {}
  for k, v in pairs(t) do r[k] = v end
  return r
end
function util.deep_copy(t)
  local seen = {}
  local function cp(o)
    if type(o) ~= "table" then return o end
    if seen[o] then return seen[o] end
    local r = {}
    seen[o] = r
    for k, v in pairs(o) do r[cp(k)] = cp(v) end
    return setmetatable(r, getmetatable(o))
  end
  return cp(t)
end
table.copy = table.copy or util.copy
table.copy_deep = table.copy_deep or util.deep_copy
copy = util.copy
copy_line = function(l) return util.copy(l) end

function util.trim(s) return (s:gsub("^%s+", ""):gsub("%s+$", "")) end
string.trim = string.trim or util.trim
function util.headtail(s)
  local a, b, head, tail = s:find("(.-)%s+(.*)")
  if a then return head, tail end
  return s, ""
end
string.headtail = string.headtail or util.headtail
function util.words(s)
  return function()
    local h, t = util.headtail(s)
    if h == "" then return nil end
    s = t
    return h
  end
end
string.words = string.words or util.words

function util.ass_color(r, g, b)
  return string.format("&H%02X%02X%02X&", util.clamp(round(b), 0, 255), util.clamp(round(g), 0, 255), util.clamp(round(r), 0, 255))
end
function util.ass_alpha(a) return string.format("&H%02X&", util.clamp(round(a), 0, 255)) end
function util.ass_style_color(r, g, b, a)
  return string.format("&H%02X%02X%02X%02X", util.clamp(round(a or 0), 0, 255), util.clamp(round(b), 0, 255), util.clamp(round(g), 0, 255), util.clamp(round(r), 0, 255))
end
function util.extract_color(s)
  local a, b, g, r = s:match("&H(%x%x)(%x%x)(%x%x)(%x%x)")
  if a then return tonumber(r, 16), tonumber(g, 16), tonumber(b, 16), tonumber(a, 16) end
  b, g, r = s:match("&H(%x%x)(%x%x)(%x%x)&?$")
  if b then return tonumber(r, 16), tonumber(g, 16), tonumber(b, 16), 0 end
  a = s:match("&H(%x%x)&?$")
  if a then return 0, 0, 0, tonumber(a, 16) end
  r, g, b = s:match("#(%x%x)(%x%x)(%x%x)")
  if r then return tonumber(r, 16), tonumber(g, 16), tonumber(b, 16), 0 end
  return 0, 0, 0, 0
end
function util.alpha_from_style(s)
  local _, _, _, a = util.extract_color(s)
  return util.ass_alpha(a)
end
function util.color_from_style(s)
  local r, g, b = util.extract_color(s)
  return util.ass_color(r, g, b)
end
function util.html_color(s)
  local r, g, b = util.extract_color(s)
  return util.ass_color(r, g, b)
end
function util.interpolate(t, a, b) return a + (b - a) * t end
function util.interpolate_color(t, c1, c2)
  local r1, g1, b1 = util.extract_color(c1)
  local r2, g2, b2 = util.extract_color(c2)
  return util.ass_color(util.interpolate(t, r1, r2), util.interpolate(t, g1, g2), util.interpolate(t, b1, b2))
end
function util.interpolate_alpha(t, a1, a2)
  local _, _, _, x1 = util.extract_color(a1)
  local _, _, _, x2 = util.extract_color(a2)
  return util.ass_alpha(util.interpolate(t, x1, x2))
end
function util.float_str(n)
  local s = string.format("%.3f", n)
  s = s:gsub("0+$", ""):gsub("%.$", "")
  if s == "-0" then s = "0" end
  return s
end
function util.fix_fontname(n) return (n:gsub("^@", "")) end
function util.uuid()
  return (("xxxxxxxx-xxxx-4xxx-yxxx-xxxxxxxxxxxx"):gsub("[xy]", function(c)
    local v = (c == "x") and math.random(0, 15) or math.random(8, 11)
    return string.format("%x", v)
  end))
end
function util.list_to_set(l) local s = {} for _, v in ipairs(l) do s[v] = true end return s end
function util.table_values(t) local r = {} for _, v in pairs(t) do r[#r + 1] = v end return r end
table.values = table.values or util.table_values

-- export as globals like the original include file
for _, name in ipairs { "ass_color", "ass_alpha", "ass_style_color", "extract_color", "alpha_from_style", "color_from_style",
                        "html_color", "interpolate", "interpolate_color", "interpolate_alpha", "float_str", "fix_fontname" } do
  if _G[name] == nil then _G[name] = util[name] end
end
if string.find == nil then end
return util
