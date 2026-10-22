-- AniSub re.lua : Aegisub `aegisub.re` compatible regex module (backed by Rust regex).
local host = __host
local re = { ICASE = 1, NOSUB = 2, NO_MOD_M = 4, MOD_S = 8, NO_MOD_S = 16, EXTENDED = 32 }

local function match_tables(str, list)
  if #list == 0 then return nil end
  local out = {}
  for i, m in ipairs(list) do
    local g = {}
    for j, c in ipairs(m) do g[j] = { str = c[1], first = c[2], last = c[3] } end
    out[i] = g
  end
  return out
end

function re.find(str, pat, flags)
  local r = host.re_find(str, pat, flags or 0)
  if not r then return nil end
  -- returns list of {str, first, last} for the first match then captures (flattened per match)
  local out = {}
  for _, m in ipairs(r) do
    for _, c in ipairs(m) do out[#out + 1] = { str = c[1], first = c[2], last = c[3] } end
  end
  return #out > 0 and out or nil
end

function re.match(str, pat, flags)
  local r = host.re_find(str, pat, flags or 0)
  if not r or not r[1] then return nil end
  local out = {}
  for j, c in ipairs(r[1]) do out[j] = { str = c[1], first = c[2], last = c[3] } end
  return out
end

function re.gfind(str, pat, flags)
  local r = host.re_find(str, pat, flags or 0) or {}
  local i = 0
  return function()
    i = i + 1
    local m = r[i]
    if not m then return nil end
    local c = m[1]
    return c[1], c[2], c[3]
  end
end

function re.sub(str, pat, repl, count, flags)
  return host.re_sub(str, pat, repl, count or 0, flags or 0)
end
re.gsub = re.sub

function re.split(str, pat, flags)
  return host.re_split(str, pat, flags or 0)
end

function re.compile(pat, flags)
  local o = {}
  function o:find(s) return re.find(s, pat, flags) end
  function o:match(s) return re.match(s, pat, flags) end
  function o:gfind(s) return re.gfind(s, pat, flags) end
  function o:sub(s, repl, count) return re.sub(s, pat, repl, count, flags) end
  o.gsub = o.sub
  return o
end

return re
