-- Offline DependencyControl (l0.DependencyControl) compatibility layer.
--
-- It implements what scripts and modules call at load time: the record constructor (with the positional
-- module list), requireModules/checkOptionalModules, register, registerMacro(s)/Filter, getLogger,
-- getConfigHandler (JSON files under ?user/config) and no-op updater/test hooks. It never touches the
-- network. Set Options::real_depctrl to use the genuine DependencyControl from the include path instead.

local DC = {}
DC.__index = DC
DC.version = "0.7.0-nightsub"

-- ------------------------------------------------------------------ tiny JSON (config persistence)
local json = {}
do
  local esc = { ['"'] = '\\"', ['\\'] = '\\\\', ['\b'] = '\\b', ['\f'] = '\\f', ['\n'] = '\\n', ['\r'] = '\\r', ['\t'] = '\\t' }
  local function enc(v, out, ind)
    local t = type(v)
    if t == "nil" then out[#out + 1] = "null"
    elseif t == "boolean" then out[#out + 1] = tostring(v)
    elseif t == "number" then
      if v ~= v or v == math.huge or v == -math.huge then out[#out + 1] = "null"
      elseif v == math.floor(v) and math.abs(v) < 1e15 then out[#out + 1] = string.format("%d", v)
      else out[#out + 1] = string.format("%.14g", v) end
    elseif t == "string" then
      out[#out + 1] = '"' .. v:gsub('[%c"\\]', function(c) return esc[c] or string.format("\\u%04x", c:byte()) end) .. '"'
    elseif t == "table" then
      local n = #v
      local is_arr = n > 0 or next(v) == nil
      if is_arr then
        for k in pairs(v) do if type(k) ~= "number" then is_arr = false break end end
      end
      if is_arr then
        out[#out + 1] = "["
        for i = 1, n do
          if i > 1 then out[#out + 1] = "," end
          enc(v[i], out)
        end
        out[#out + 1] = "]"
      else
        out[#out + 1] = "{"
        local first = true
        local keys = {}
        for k in pairs(v) do keys[#keys + 1] = k end
        table.sort(keys, function(a, b) return tostring(a) < tostring(b) end)
        for _, k in ipairs(keys) do
          if type(v[k]) ~= "function" and type(v[k]) ~= "userdata" then
            if not first then out[#out + 1] = "," end
            first = false
            enc(tostring(k), out)
            out[#out + 1] = ":"
            enc(v[k], out)
          end
        end
        out[#out + 1] = "}"
      end
    else out[#out + 1] = "null" end
  end
  function json.encode(v) local o = {} enc(v, o) return table.concat(o) end

  local function skip(s, i) return s:find("%S", i) or #s + 1 end
  local dec
  local function str(s, i)
    local out, j = {}, i + 1
    while true do
      local c = s:sub(j, j)
      if c == "" then error("unterminated string") end
      if c == '"' then return table.concat(out), j + 1 end
      if c == "\\" then
        local n = s:sub(j + 1, j + 1)
        local map = { b = "\b", f = "\f", n = "\n", r = "\r", t = "\t", ['"'] = '"', ["\\"] = "\\", ["/"] = "/" }
        if n == "u" then
          local cp = tonumber(s:sub(j + 2, j + 5), 16)
          out[#out + 1] = (cp < 128) and string.char(cp) or (cp < 2048 and string.char(192 + math.floor(cp / 64), 128 + cp % 64)
            or string.char(224 + math.floor(cp / 4096), 128 + math.floor(cp / 64) % 64, 128 + cp % 64))
          j = j + 6
        else out[#out + 1] = map[n] or n; j = j + 2 end
      else out[#out + 1] = c; j = j + 1 end
    end
  end
  function dec(s, i)
    i = skip(s, i)
    local c = s:sub(i, i)
    if c == "{" then
      local t = {}
      i = skip(s, i + 1)
      if s:sub(i, i) == "}" then return t, i + 1 end
      while true do
        local k; k, i = str(s, skip(s, i))
        i = skip(s, i)
        assert(s:sub(i, i) == ":", "expected ':'")
        local v; v, i = dec(s, i + 1)
        t[k] = v
        i = skip(s, i)
        local d = s:sub(i, i)
        if d == "}" then return t, i + 1 end
        assert(d == ",", "expected ','")
        i = i + 1
      end
    elseif c == "[" then
      local t, n = {}, 0
      i = skip(s, i + 1)
      if s:sub(i, i) == "]" then return t, i + 1 end
      while true do
        local v; v, i = dec(s, i)
        n = n + 1; t[n] = v
        i = skip(s, i)
        local d = s:sub(i, i)
        if d == "]" then return t, i + 1 end
        assert(d == ",", "expected ','")
        i = i + 1
      end
    elseif c == '"' then return str(s, i)
    elseif s:sub(i, i + 3) == "true" then return true, i + 4
    elseif s:sub(i, i + 4) == "false" then return false, i + 5
    elseif s:sub(i, i + 3) == "null" then return nil, i + 4
    else
      local num = s:match("^-?%d+%.?%d*[eE]?[+-]?%d*", i)
      assert(num and #num > 0, "bad json at " .. i)
      return tonumber(num), i + #num
    end
  end
  function json.decode(s) local v = dec(s, 1) return v end
end
DC.json = json

-- ------------------------------------------------------------------ Logger
local Logger = {}
Logger.__index = Logger
Logger.LogLevels = { Fatal = 0, Error = 1, Warning = 2, Hint = 3, Debug = 4, Trace = 5 }
local level_names = { [0] = "Fatal", "Error", "Warning", "Hint", "Debug", "Trace" }

local function new_logger(opts)
  opts = opts or {}
  local self = setmetatable({}, Logger)
  self.defaultLevel = opts.defaultLevel or 2
  self.maxToFileLevel = opts.maxToFileLevel or 4
  self.logLevel = opts.logLevel or 3
  self.prefix = opts.prefix or ""
  self.toFile = false
  self.toWindow = true
  self.indent = 0
  return self
end

function Logger:logEx(level, msg, insertLineBreak, prefix, ...)
  if type(level) ~= "number" then level = self.defaultLevel end
  if level > self.logLevel then return end
  if select("#", ...) > 0 then msg = msg:format(...) end
  local p = (prefix ~= false) and (self.prefix ~= "" and (self.prefix .. " ") or "") or ""
  local lvl = level_names[level] or "Log"
  aegisub.log(string.format("%s%s%s%s", p, (level <= 2) and ("[" .. lvl .. "] ") or "", string.rep("  ", self.indent) .. tostring(msg), insertLineBreak == false and "" or "\n"))
  return msg
end
function Logger:log(a, ...)
  if type(a) == "number" then return self:logEx(a, ...) end
  return self:logEx(self.defaultLevel, a, true, nil, ...)
end
for _, name in ipairs { "Fatal", "Error", "Warning", "Hint", "Debug", "Trace" } do
  local lvl = Logger.LogLevels[name]
  local fn = function(self, msg, ...) return self:logEx(lvl, tostring(msg), true, nil, ...) end
  Logger[name:lower()] = fn
end
Logger.warn = Logger.warning
function Logger:assert(cond, msg, ...)
  if not cond then
    local m = select("#", ...) > 0 and tostring(msg):format(...) or tostring(msg)
    self:logEx(1, m)
    error(m, 2)
  end
  return cond
end
function Logger:dump(obj, ...)
  local function d(v, depth)
    if type(v) ~= "table" or depth > 4 then return tostring(v) end
    local parts = {}
    for k, x in pairs(v) do parts[#parts + 1] = tostring(k) .. " = " .. d(x, depth + 1) end
    return "{ " .. table.concat(parts, ", ") .. " }"
  end
  return self:logEx(self.defaultLevel, d(obj, 0))
end
function Logger:progress(...) end
function Logger:indent_(n) self.indent = self.indent + n end
function Logger:setLevel(l) self.logLevel = l end
function Logger:format(msg, indent, ...) return msg:format(...) end
DC.Logger = setmetatable({}, { __call = function(_, opts) return new_logger(opts) end, __index = Logger })

-- ------------------------------------------------------------------ ConfigHandler (JSON under ?user/config)
local ConfigHandler = {}
ConfigHandler.__index = ConfigHandler

local function config_dir()
  local d = aegisub.decode_path("?user") .. "/config"
  return d
end

local function copy_into(dst, src)
  for k, v in pairs(src) do
    if type(v) == "table" and type(dst[k]) == "table" then copy_into(dst[k], v)
    elseif type(v) == "table" then dst[k] = aegisub.util_copy_deep and aegisub.util_copy_deep(v) or (function() local c = {} copy_into(c, v) return c end)()
    else dst[k] = v end
  end
end

function ConfigHandler.new(owner, defaults, section, noLoad, configDir)
  local self = setmetatable({}, ConfigHandler)
  self.owner = owner
  self.defaults = defaults or {}
  self.section = section
  self.file = (configDir or config_dir()) .. "/" .. (owner.namespace or owner.name or "script") .. ".json"
  self.userConfig = {}
  self.c = {}
  copy_into(self.c, self.defaults)
  if not noLoad then self:load() end
  return self
end

function ConfigHandler:load()
  local f = io.open(self.file, "rb")
  self.c = {}
  copy_into(self.c, self.defaults)
  if f then
    local data = f:read("*a")
    f:close()
    local ok, t = pcall(json.decode, data)
    if ok and type(t) == "table" then
      local sect = t
      if self.section then
        for part in tostring(self.section):gmatch("[^%.]+") do sect = type(sect) == "table" and sect[part] or nil end
      end
      if type(sect) == "table" then copy_into(self.c, sect) end
    end
  end
  return self.c
end

function ConfigHandler:write()
  local all = {}
  local f = io.open(self.file, "rb")
  if f then
    local ok, t = pcall(json.decode, f:read("*a"))
    f:close()
    if ok and type(t) == "table" then all = t end
  end
  local target = all
  if self.section then
    for part in tostring(self.section):gmatch("[^%.]+") do
      target[part] = type(target[part]) == "table" and target[part] or {}
      target = target[part]
    end
    for k in pairs(target) do target[k] = nil end
  else
    for k in pairs(target) do target[k] = nil end
  end
  copy_into(target, self.c)
  local lfs = require("aegisub.lfs")
  lfs.mkdir(aegisub.decode_path("?user"))
  lfs.mkdir(config_dir())
  local out = io.open(self.file, "wb")
  if not out then return false, "cannot write " .. self.file end
  out:write(json.encode(all))
  out:close()
  return true
end
ConfigHandler.save = ConfigHandler.write
function ConfigHandler:import(t, keys) copy_into(self.c, t) return self end
function ConfigHandler:delete() os.remove(self.file) return true end
DC.ConfigHandler = setmetatable({}, { __call = function(_, ...) return ConfigHandler.new(...) end })

-- ------------------------------------------------------------------ the DependencyControl record
local function parse_version(v)
  if type(v) == "number" then return v end
  if type(v) ~= "string" then return 0 end
  local a, b, c = v:match("^(%d+)%.?(%d*)%.?(%d*)")
  return (tonumber(a) or 0) * 65536 + (tonumber(b) or 0) * 256 + (tonumber(c) or 0)
end

local function module_name(m)
  if type(m) == "table" then return m[1] or m.moduleName, m end
  return m, {}
end

local function new(_, record)
  record = record or {}
  local self = setmetatable({}, DC)
  self.name = record.name or record.scriptName or script_name or "?"
  self.scriptName, self.moduleName = self.name, record.moduleName
  self.namespace = record.namespace or record.moduleName or self.name
  self.version = record.version or "0.0.0"
  self.versionNumber = parse_version(self.version)
  self.description, self.author, self.url, self.feed = record.description, record.author, record.url, record.feed
  self.configFile = record.configFile
  self.scriptType = record.moduleName and 2 or 1
  self.requiredModules = record[1] or record.requiredModules or {}
  self.depRecord = record
  self.logger = new_logger{ prefix = "[" .. self.name .. "]" }
  self.updater = setmetatable({}, { __index = function() return function() return false end end })
  return self
end

function DC:getVersionNumber() return self.versionNumber end
function DC:getVersionString() return tostring(self.version) end
DC.getVersion = DC.getVersionString
function DC:checkVersion(v) return parse_version(v) <= self.versionNumber end
function DC:getLogger(opts)
  local l = new_logger(opts)
  l.prefix = (opts and opts.prefix) or ("[" .. self.name .. "]")
  return l
end
function DC:getConfigHandler(defaults, section, noLoad) return ConfigHandler.new(self, defaults, section, noLoad) end
function DC:getConfigFileName() return self.configFile or (self.namespace .. ".json") end

function DC:requireModules(modules)
  modules = modules or self.requiredModules
  local out, n = {}, 0
  for i, m in ipairs(modules) do
    local name, spec = module_name(m)
    local ok, mod = pcall(require, name)
    if not ok then
      if spec.optional then mod = nil
      else error(string.format("DependencyControl shim: required module '%s' (needed by %s) could not be loaded:\n%s", tostring(name), self.name, tostring(mod)), 2) end
    end
    n = i
    out[i] = mod
  end
  return unpack(out, 1, n)
end

function DC:checkOptionalModules(modules)
  if type(modules) == "string" then modules = { modules } end
  local missing = {}
  for _, m in ipairs(modules or {}) do
    local name = module_name(m)
    if not pcall(require, name) then missing[#missing + 1] = name end
  end
  if #missing > 0 then
    return false, "Optional modules missing: " .. table.concat(missing, ", ")
  end
  return true
end

function DC:register(ref)
  if type(ref) == "table" then ref.version = self end
  return ref or self
end

function DC:registerMacro(name, desc, fn, validate, active)
  if type(name) == "function" then
    name, desc, fn, validate, active = self.name, self.description, name, desc, fn
  elseif type(desc) == "function" then
    name, desc, fn, validate, active = self.name, name, desc, fn, validate
  end
  aegisub.register_macro(name, desc or "", fn, validate, active)
end
function DC:registerMacros(macros, useSubmenu)
  for _, m in ipairs(macros) do
    local name, desc, fn, validate, active = m[1] or m.name, m[2] or m.description, m[3] or m.fn, m[4] or m.validate, m[5] or m.active
    local full = useSubmenu and (self.name .. "/" .. name) or name
    aegisub.register_macro(full, desc or "", fn, validate, active)
  end
end
function DC:registerFilter(...) aegisub.register_filter(...) end
function DC:registerTests() end
function DC:getUpdater() return self.updater end
function DC:unlock() end
function DC:getFeedLocation() return self.feed end

-- Used by test suites in modules: harmless stand-ins
local noop_class = setmetatable({}, { __call = function() return setmetatable({}, { __index = function() return function() end end }) end,
  __index = function() return function() end end })
DC.UnitTestSuite = noop_class
DC.UnitTestSuite.UnitTest = noop_class
DC.Updater = DC.updater
DC.ConfigView = noop_class
DC.Utils = { }
DC.Environment = {}
DC.Domain = {}

return setmetatable(DC, { __call = new })
