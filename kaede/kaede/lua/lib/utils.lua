-- utils.lua: legacy Aegisub helper extensions (string/table/math)
local util = require "util"

function string.trim(s) return (s:gsub("^%s+", ""):gsub("%s+$", "")) end
function string.headtail(s) return util.headtail(s) end
function string.words(s) return util.words(s) end
function string.starts_with(s, p) return s:sub(1, #p) == p end
function string.ends_with(s, p) return p == "" or s:sub(-#p) == p end
function math.round(x, dec)
  local m = 10 ^ (dec or 0)
  return math.floor(x * m + 0.5) / m
end
function table.copy(t) return util.copy(t) end
function table.copy_deep(t) return util.deep_copy(t) end
function table.join(...)
  local r = {}
  for _, t in ipairs({...}) do for _, v in ipairs(t) do r[#r + 1] = v end end
  return r
end
function table.values(t) local r = {} for _, v in pairs(t) do r[#r + 1] = v end return r end
function table.keys(t) local r = {} for k in pairs(t) do r[#r + 1] = k end return r end
function table.reverse(t) local r = {} for i = #t, 1, -1 do r[#r + 1] = t[i] end return r end
function table.contains(t, x) for _, v in ipairs(t) do if v == x then return true end end return false end
return util
