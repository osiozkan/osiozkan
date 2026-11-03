-- util.lua (Aegisub-compatible subset + extras)
local util = {}

function util.copy(t)
  local r = {}
  for k, v in pairs(t) do r[k] = v end
  return r
end

function util.deep_copy(t, seen)
  if type(t) ~= "table" then return t end
  seen = seen or {}
  if seen[t] then return seen[t] end
  local r = {}
  seen[t] = r
  for k, v in pairs(t) do r[util.deep_copy(k, seen)] = util.deep_copy(v, seen) end
  return setmetatable(r, getmetatable(t))
end

function util.clamp(v, lo, hi) return math.max(lo, math.min(hi, v)) end

function util.ass_color(r, g, b)
  return string.format("&H%02X%02X%02X&", util.clamp(math.floor(b + 0.5), 0, 255), util.clamp(math.floor(g + 0.5), 0, 255), util.clamp(math.floor(r + 0.5), 0, 255))
end
function util.ass_alpha(a) return string.format("&H%02X&", util.clamp(math.floor(a + 0.5), 0, 255)) end
function util.ass_style_color(r, g, b, a)
  return string.format("&H%02X%02X%02X%02X", a or 0, b, g, r)
end

function util.extract_color(s)
  local a, b, g, r
  s = tostring(s)
  -- &HAABBGGRR (style) , &HBBGGRR& (tag) , decimal
  a, b, g, r = s:match("&[Hh](%x%x)(%x%x)(%x%x)(%x%x)")
  if a then return tonumber(r, 16), tonumber(g, 16), tonumber(b, 16), tonumber(a, 16) end
  b, g, r = s:match("&[Hh](%x%x)(%x%x)(%x%x)&?")
  if b then return tonumber(r, 16), tonumber(g, 16), tonumber(b, 16), 0 end
  local n = tonumber(s)
  if n then return n % 256, math.floor(n / 256) % 256, math.floor(n / 65536) % 256, math.floor(n / 16777216) % 256 end
  return nil
end

function util.alpha_from_style(s)
  local _, _, _, a = util.extract_color(s)
  return util.ass_alpha(a or 0)
end
function util.color_from_style(s)
  local r, g, b = util.extract_color(s)
  return util.ass_color(r or 0, g or 0, b or 0)
end

function util.interpolate(pct, a, b) return a + (b - a) * pct end
function util.interpolate_color(pct, c1, c2)
  local r1, g1, b1 = util.extract_color(c1)
  local r2, g2, b2 = util.extract_color(c2)
  return util.ass_color(util.interpolate(pct, r1, r2), util.interpolate(pct, g1, g2), util.interpolate(pct, b1, b2))
end
function util.interpolate_alpha(pct, a1, a2)
  local _, _, _, x = util.extract_color(a1 .. "")
  local _, _, _, y = util.extract_color(a2 .. "")
  local pa = tonumber(tostring(a1):match("%x%x"), 16) or x or 0
  local pb = tonumber(tostring(a2):match("%x%x"), 16) or y or 0
  return util.ass_alpha(util.interpolate(pct, pa, pb))
end

function util.fix_fontname(n)
  n = tostring(n)
  if n:sub(1, 1) == "@" then n = n:sub(2) end
  return n
end

function util.trim(s) return (s:gsub("^%s+", ""):gsub("%s+$", "")) end
function util.headtail(s)
  local a, b = s:match("^(%S*)%s*(.*)$")
  return a, b
end
function util.words(s)
  return function()
    if s == "" then return nil end
    local sp, w, rest = s:match("^(%s*)(%S+)(.*)$")
    if not w then local t = s; s = ""; return t, "" end
    s = rest
    return w, sp
  end
end

function util.uuid()
  return string.format("%08x-%04x-4%03x-a%03x-%012x", math.random(0, 0xffffffff), math.random(0, 0xffff), math.random(0, 0xfff), math.random(0, 0xfff), math.random(0, 0xffffffffffff))
end

function util.progress_string(...) return string.format(...) end

-- Kaede extras --------------------------------------------------------
function util.ms_to_ass(ms)
  ms = math.max(0, ms)
  local cs = math.floor((ms + 5) / 10)
  return string.format("%d:%02d:%02d.%02d", math.floor(cs / 360000), math.floor(cs / 6000) % 60, math.floor(cs / 100) % 60, cs % 100)
end

return util
