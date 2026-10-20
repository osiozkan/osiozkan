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

-- ---------------------------------------------------------------- include / require
local function read_file(path)
  local f = io.open(path, "rb")
  if not f then return nil end
  local s = f:read("*a")
  f:close()
  return s
end

local function load_embedded(name)
  local src = __nsub_embedded[name]
  if not src then return nil end
  local fn, err = loadstring(src, "=" .. name)
  if not fn then error(err) end
  return fn
end

-- Aliases: include("util.lua") / include("utils.lua") / require "aegisub.util" ...
local alias = {
  ["util"] = "utils", ["utils"] = "utils", ["aegisub.util"] = "utils",
  ["karaskel"] = "karaskel", ["unicode"] = "unicode", ["aegisub.unicode"] = "unicode",
  ["l0.DependencyControl"] = "l0.DependencyControl",
}

for modname, emb in pairs(alias) do
  package.preload[modname] = function()
    local fn = load_embedded(emb)
    local r = fn(modname)
    return r
  end
end

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

function include(name)
  local base = name:gsub("%.lua$", "")
  local p = find_in_dirs(name)
  if p then
    local fn, err = loadfile(p)
    if not fn then error(err, 0) end
    return fn()
  end
  if alias[base] then
    local fn = load_embedded(alias[base])
    return fn(base)
  end
  error("Lua include not found: " .. name, 2)
end

-- Aegisub exposes these helpers globally when utils.lua is included; we keep them lazy:
-- the scripts that need them call include("utils.lua") first, exactly as under Aegisub.

-- ---------------------------------------------------------------- re module
local re = {}
re.ICASE, re.NO_MOD_M, re.MOD_S, re.MOD_X, re.NOSUB, re.NEWLINE = 1, 2, 4, 8, 16, 32
local function mkflags(f) return type(f) == "number" and f or 0 end

local Regex = {}
Regex.__index = Regex
function re.compile(pattern, flags)
  return setmetatable({ pattern = pattern, flags = mkflags(flags) }, Regex)
end
function Regex:match(str, flags)
  local m = __re_matches(self.pattern, self.flags, str, 1)
  return m[1]
end
function Regex:find(str, flags)
  local all = __re_matches(self.pattern, self.flags, str)
  if #all == 0 then return nil end
  local out = {}
  for i, m in ipairs(all) do out[i] = m[1] end
  return out
end
function Regex:gmatch(str)
  local all = __re_matches(self.pattern, self.flags, str)
  local i = 0
  return function() i = i + 1; return all[i] and all[i][1] end
end
function Regex:sub(str, repl, count)
  if type(repl) == "function" then
    local all = __re_matches(self.pattern, self.flags, str, count)
    local out, last = {}, 1
    for _, m in ipairs(all) do
      local w = m[1]
      out[#out + 1] = str:sub(last, w.first - 1)
      out[#out + 1] = repl(w.str) or ""
      last = w.last + 1
    end
    out[#out + 1] = str:sub(last)
    return table.concat(out)
  end
  return __re_sub(self.pattern, self.flags, str, repl, count)
end
Regex.gsub = Regex.sub
function Regex:split(str, skip_empty, max)
  return __re_split(self.pattern, self.flags, str, skip_empty, max)
end

function re.match(pattern, str, flags) return re.compile(pattern, flags):match(str) end
function re.find(str, pattern, flags) return re.compile(pattern, flags):find(str) end
function re.gmatch(str, pattern, flags) return re.compile(pattern, flags):gmatch(str) end
function re.sub(str, pattern, repl, count, flags) return re.compile(pattern, flags):sub(str, repl, count) end
re.gsub = re.sub
function re.split(str, sep, skip_empty, max, flags) return re.compile(sep, flags):split(str, skip_empty, max) end
function re.escape(s) return (s:gsub("[%^%$%(%)%%%.%[%]%*%+%-%?{}|\\]", "\\%0")) end
package.preload["re"] = function() return re end
package.preload["aegisub.re"] = function() return re end

-- ---------------------------------------------------------------- lfs (minimal)
local lfs = {}
function lfs.attributes(path, attr)
  local f = io.open(path, "rb")
  if not f then return nil, "No such file or directory" end
  local size = f:seek("end")
  f:close()
  local t = { mode = "file", size = size }
  if attr then return t[attr] end
  return t
end
function lfs.currentdir() return "." end
package.preload["lfs"] = function() return lfs end
