-- Kitsune Aegisub-compat prelude. `__k` is the Python bridge table.
local K = __k

local function to_lua_line(raw) return raw end

-- subtitle collection proxy (1-based, like Aegisub)
local subs = newproxy(true)
local methods = {}
function methods.insert(self, idx, ...)
  local items = {...}
  for i, t in ipairs(items) do K.insert(idx + i - 1, t) end
end
function methods.append(self, ...)
  local items = {...}
  for _, t in ipairs(items) do K.insert(K.count() + 1, t) end
end
function methods.delete(self, ...)
  local args = {...}
  if type(args[1]) == "table" then args = args[1] end
  table.sort(args, function(a, b) return a > b end)
  for _, i in ipairs(args) do K.delete(i) end
end
function methods.deleterange(self, a, b)
  for i = b, a, -1 do K.delete(i) end
end
function methods.n(self) return K.count() end
local function seq_iter(self) return function(_, i) i = i + 1; if i <= K.count() then return i, K.get(i) end end, self, 0 end
local mt = getmetatable(subs)
for k, v in pairs({
  __index = function(t, k)
    if type(k) == "number" then
      if k < 1 or k > K.count() then return nil end
      return K.get(k)
    elseif k == "n" then return K.count()
    end
    return methods[k]
  end,
  __newindex = function(t, k, v)
    if type(k) == "number" then K.set(k, v) end
  end,
  __len = function() return K.count() end,
  __call = function(self) return seq_iter(self) end,
}) do mt[k] = v end
-- iterate: `for i = 1, #subs` or `for i, l in subs:iter()`
function methods.iter(self) return seq_iter(self) end
function methods.ipairs(self) return seq_iter(self) end
__subs = subs
