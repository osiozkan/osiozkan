-- Aegisub utils.lua (compatible subset + common helpers)
function string.headtail(s)
  local a, b, head, tail = s:find("(.-)%s+(.*)")
  if a then return head, tail end
  return s, ""
end
function string.trim(s) return (s:gsub("^%s+", ""):gsub("%s+$", "")) end
function string.words(s) local r = {} for w in s:gmatch("%S+") do r[#r + 1] = w end return r end
function string.startswith(s, p) return s:sub(1, #p) == p end
function string.endswith(s, p) return p == "" or s:sub(-#p) == p end
function string.split(s, sep, plain)
  local r, pos = {}, 1
  while true do
    local a, b = s:find(sep, pos, plain ~= false)
    if not a then r[#r + 1] = s:sub(pos) break end
    r[#r + 1] = s:sub(pos, a - 1); pos = b + 1
  end
  return r
end
function table.copy(t) local r = {} for k, v in pairs(t) do r[k] = v end return r end
function table.copy_deep(t)
  if type(t) ~= "table" then return t end
  local r = {} for k, v in pairs(t) do r[k] = table.copy_deep(v) end
  return setmetatable(r, getmetatable(t))
end
function table.append(a, b) for _, v in ipairs(b) do a[#a + 1] = v end return a end
function table.indexof(t, v) for i, x in ipairs(t) do if x == v then return i end end end
function math.round(n, d) local m = 10 ^ (d or 0) return floor and floor(n * m + 0.5) / m or math.floor(n * m + 0.5) / m end
function math.clamp(v, a, b) return math.max(a, math.min(b, v)) end

function clamp(v, lo, hi) if v < lo then return lo elseif v > hi then return hi end return v end
function interpolate(pct, a, b) return a + (b - a) * pct end
function interpolate_color(pct, c1, c2)
  local r1, g1, b1 = extract_color(c1); local r2, g2, b2 = extract_color(c2)
  return ass_color(math.floor(interpolate(pct, r1, r2) + .5), math.floor(interpolate(pct, g1, g2) + .5), math.floor(interpolate(pct, b1, b2) + .5))
end
function interpolate_alpha(pct, a1, a2)
  local x = math.floor(interpolate(pct, a1, a2) + .5)
  return string.format("&H%02X&", x)
end
function ass_alpha(a) return string.format("&H%02X&", a) end
function ass_color(r, g, b) return string.format("&H%02X%02X%02X&", b, g, r) end
function ass_style_color(r, g, b) return string.format("&H00%02X%02X%02X", b, g, r) end
function extract_color(s)
  local a, b, g, r = s:match("&H(%x%x)(%x%x)(%x%x)(%x%x)")
  if r then return tonumber(r, 16), tonumber(g, 16), tonumber(b, 16), tonumber(a, 16) end
  b, g, r = s:match("&H(%x%x)(%x%x)(%x%x)&?")
  if r then return tonumber(r, 16), tonumber(g, 16), tonumber(b, 16) end
  local h = s:match("#(%x%x%x%x%x%x)")
  if h then return tonumber(h:sub(1, 2), 16), tonumber(h:sub(3, 4), 16), tonumber(h:sub(5, 6), 16) end
  return 0, 0, 0
end
function alpha_from_style(s) local a = s:match("&H(%x%x)%x%x%x%x%x%x") return a and ("&H" .. a .. "&") or "&H00&" end
function color_from_style(s) local b, g, r = s:match("&H%x%x(%x%x)(%x%x)(%x%x)") return b and ("&H" .. b .. g .. r .. "&") or "&H000000&" end
function html_color(r, g, b) return string.format("#%02X%02X%02X", r, g, b) end
function ass_color_from_html(h) return ass_color(extract_color(h)) end
function retime(mode, line, start_offset, end_offset) end
function list_to_table(l) return l end
function fold_case(s) return s:lower() end
