-- nightsub prelude: Aegisub-compatible globals implemented in Lua.

local function fmt(...)
  local n = select('#', ...)
  if n == 0 then return "" end
  local first = ...
  if n == 1 then return tostring(first) end
  local ok, s = pcall(string.format, ...)
  if ok then return s end
  return tostring(first)
end

-- aegisub.log([level,] fmt, ...)  /  aegisub.debug.out([level,] fmt, ...)
local function logger(...)
  local a = ...
  if type(a) == "number" then
    __nsub_log(fmt(select(2, ...)))
  else
    __nsub_log(fmt(...))
  end
end
aegisub.log = logger
aegisub.debug = { out = logger }
aegisub.debug.warn = logger
aegisub.debug.error = logger
local real_is_cancelled = aegisub.is_cancelled
aegisub.progress = {
  set = function(v) __nsub_progress("set", tonumber(v) or 0, nil) end,
  task = function(...) __nsub_progress("task", nil, fmt(...)) end,
  title = function(...) __nsub_progress("title", nil, fmt(...)) end,
  is_cancelled = function() return real_is_cancelled() end,
}
aegisub.unicode = nil

-- Aegisub's subs object supports ipairs(); plain LuaJIT's ipairs only takes tables.
do
  local real_ipairs = ipairs
  function ipairs(t)
    if type(t) == "userdata" then
      local i = 0
      return function()
        i = i + 1
        if i > #t then return nil end
        return i, t[i]
      end, t, 0
    end
    return real_ipairs(t)
  end
  table.unpack = table.unpack or unpack
end

-- ---------------------------------------------------------------- include / require
-- Embedded sources: "include/<file>.lua" are Aegisub's stock include files, "aegisub/<mod>" our module ports.
local function load_embedded(key)
  local src = __nsub_embedded[key]
  if not src then return nil end
  src = src:gsub("^\239\187\191", "") -- strip UTF-8 BOM
  local fn, err = loadstring(src, "=" .. key)
  if not fn then error(err) end
  return fn
end

for _, m in ipairs { "util", "unicode", "re", "lfs", "clipboard", "argcheck", "ffi" } do
  package.preload["aegisub." .. m] = function(name)
    return load_embedded("aegisub/" .. m)(name)
  end
end
package.preload["l0.DependencyControl"] = function(name) return load_embedded("depctrl")(name) end

-- require "karaskel" / "utils" / "cleantags" ... resolve against the embedded include directory.
table.insert(package.loaders, 2, function(name)
  local fn = load_embedded("include/" .. name .. ".lua")
  if fn then return fn end
  return nil
end)
-- Aegisub also allows require "aegisub.util" style names for stock include files.

local function find_in_dirs(name)
  local candidates = { name }
  if not name:match("%.lua$") then candidates[#candidates + 1] = name .. ".lua" end
  for _, dir in ipairs(__nsub_include_dirs) do
    for _, c in ipairs(candidates) do
      local p = dir .. "/" .. c
      local f = io.open(p, "rb")
      if f then f:close() return p end
    end
  end
  for dir in package.path:gmatch("[^;]+") do
    for _, c in ipairs(candidates) do
      local p = dir:gsub("%?.*$", "") .. c
      local f = io.open(p, "rb")
      if f then f:close() return p end
    end
  end
end

-- include("file.lua"): search include dirs / script dir, then the embedded stock files.
function include(name)
  local p = find_in_dirs(name)
  if p then
    local fn, err = loadfile(p)
    if not fn then error(err, 0) end
    return fn()
  end
  local key = name:match("%.lua$") and name or (name .. ".lua")
  local fn = load_embedded("include/" .. key)
  if fn then return fn(name) end
  error("Lua include not found: " .. name, 2)
end
