-- aegisub.unicode: port of Aegisub's unicode.moon. Case mapping is done natively (full Unicode).
local M = {}

function M.charwidth(s, i)
  local b = s:byte(i or 1)
  if not b then return 1 elseif b < 128 then return 1 elseif b < 224 then return 2 elseif b < 240 then return 3 else return 4 end
end

function M.chars(s)
  local curchar, i = 0, 1
  return function()
    if i > s:len() then return end
    local j = i
    curchar = curchar + 1
    i = i + M.charwidth(s, i)
    return s:sub(j, i - 1), curchar
  end
end

function M.len(s)
  local n = 0
  for _ in M.chars(s) do n = n + 1 end
  return n
end

function M.codepoint(s)
  local b = s:byte(1)
  if b < 128 then return b end
  local res, w
  if b < 224 then res = b - 192; w = 2
  elseif b < 240 then res = b - 224; w = 3
  else res = b - 240; w = 4 end
  for i = 2, w do res = res * 64 + s:byte(i) - 128 end
  return res
end

M.to_upper_case = function(s) return __nsub_case("upper", s) end
M.to_lower_case = function(s) return __nsub_case("lower", s) end
M.to_fold_case = function(s) return __nsub_case("fold", s) end

-- nightsub extras (not part of Aegisub): Turkish-aware casing and substring helpers.
M.upper_tr = function(s) return __nsub_case("upper_tr", s) end
M.lower_tr = function(s) return __nsub_case("lower_tr", s) end

return M
