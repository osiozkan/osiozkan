-- Make `subs` iterable with ipairs/pairs (userdata is rejected by stock LuaJIT ipairs).
local raw_ipairs, raw_pairs = ipairs, pairs
local function sub_iter(t, i)
  i = i + 1
  if i <= #t then return i, t[i] end
end
function ipairs(t)
  if type(t) == "userdata" then return sub_iter, t, 0 end
  return raw_ipairs(t)
end
function pairs(t)
  if type(t) == "userdata" then return sub_iter, t, 0 end
  return raw_pairs(t)
end
