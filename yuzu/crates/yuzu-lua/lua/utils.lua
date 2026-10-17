-- include/utils.lua (Aegisub-compatible subset)
function string.headtail(s)
  local a, b = s:find("%s+")
  if a then return s:sub(1, a - 1), s:sub(b + 1) end
  return s, ""
end

function string.trim(s) return (s:gsub("^%s+", ""):gsub("%s+$", "")) end

function string.words(s)
  return function()
    local w
    w, s = string.headtail(s)
    if w == "" and s == "" then return nil end
    return w
  end
end

function table.copy(t)
  local r = {}
  for k, v in pairs(t) do r[k] = v end
  return r
end

function table.values(t)
  local i = 0
  return function() i = i + 1; return t[i] end
end

function table.keys(t)
  local r = {}
  for k in pairs(t) do r[#r + 1] = k end
  return r
end

function table.append(a, b)
  for _, v in ipairs(b) do a[#a + 1] = v end
  return a
end

function math.round(v, d)
  d = d or 0
  local m = 10 ^ d
  return math.floor(v * m + 0.5) / m
end

function table.sort_stable(t, cmp)
  for i, v in ipairs(t) do t[i] = { v, i } end
  table.sort(t, function(a, b)
    if cmp then
      if cmp(a[1], b[1]) then return true end
      if cmp(b[1], a[1]) then return false end
    elseif a[1] ~= b[1] then
      return a[1] < b[1]
    end
    return a[2] < b[2]
  end)
  for i, v in ipairs(t) do t[i] = v[1] end
end

-- Selection helper used by many scripts
function aegisub_selection_to_set(sel)
  local s = {}
  for _, i in ipairs(sel) do s[i] = true end
  return s
end

return {}
