-- aegisub.re: regex via the host (JS RegExp, ECMAScript/PCRE-like syntax)
local re = {}
local function H(name, ...) local r = __host(name, __json.encode({ ... })); if r == '' then return nil end; return __json.decode(r) end
local function conv_matches(ms) -- ms: list of {str, first, last} 1-based byte positions
  if not ms then return nil end
  local out = {}
  for i, m in ipairs(ms) do out[i] = { str = m[1], first = m[2], last = m[3] } end
  return out
end
local function flagnum(f) return f or 0 end
function re.match(pat, str, flags) return conv_matches(H('re_match', pat, str, flagnum(flags))) end
function re.find(pat, str, flags)
  local all = H('re_find', pat, str, flagnum(flags)); if not all or #all == 0 then return nil end
  local out = {}; for i, g in ipairs(all) do out[i] = { str = g[1], first = g[2], last = g[3] } end
  return out
end
function re.sub(pat, repl, str, count, flags) return (H('re_sub', pat, repl, str, count or 0, flagnum(flags))) end
function re.gsub(pat, repl, str, flags) return re.sub(pat, repl, str, 0, flags) end
function re.split(pat, str, flags) return H('re_split', pat, str, flagnum(flags)) end
function re.compile(pat, flags)
  local c = { pattern = pat, flags = flags }
  function c:match(s) return re.match(pat, s, flags) end
  function c:find(s) return re.find(pat, s, flags) end
  function c:sub(r, s, n) return re.sub(pat, r, s, n, flags) end
  function c:gsub(r, s) return re.gsub(pat, r, s, flags) end
  function c:split(s) return re.split(pat, s, flags) end
  return c
end
re.flags = { ICASE = 1, NOSUB = 2, EXTENDED = 4 }
re.ICASE, re.NOSUB, re.EXTENDED = 1, 2, 4
return re
