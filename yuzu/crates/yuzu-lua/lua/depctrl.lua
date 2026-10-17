-- Minimal DependencyControl shim: lets scripts that `require("l0.DependencyControl")` load and run offline.
local DC = {}
DC.__index = DC

local function mk_logger(name)
  local L = { name = name, level = 3 }
  local function emit(prefix)
    return function(self, level_or_msg, msg, ...)
      if type(self) ~= "table" then return end
      local text
      if type(level_or_msg) == "number" then text = msg and string.format(tostring(msg), ...) or "" else text = string.format(tostring(level_or_msg), msg, ...) end
      aegisub.debug.out(1, string.format("[%s] %s%s\n", name or "DC", prefix, text))
    end
  end
  L.log = emit(""); L.warn = emit("Warning: "); L.error = emit("Error: "); L.hint = emit("Hint: ")
  L.trace = function() end; L.debug = function() end; L.assert = function(self, cond, msg, ...) if not cond then error(string.format(msg or "assertion failed", ...), 2) end end
  return L
end

local function parse_version(v)
  if type(v) == "number" then return v end
  if type(v) ~= "string" then return 0 end
  local a, b, c = v:match("^(%d+)%.?(%d*)%.?(%d*)")
  return (tonumber(a) or 0) * 65536 + (tonumber(b) or 0) * 256 + (tonumber(c) or 0)
end

setmetatable(DC, {
  __call = function(cls, args)
    args = args or {}
    local self = setmetatable({}, cls)
    self.name = args.name or args.moduleName or rawget(_G, "script_name") or "Script"
    self.scriptName = self.name
    self.moduleName = args.moduleName
    self.version = args.version or "0.0.0"
    self.versionNumber = parse_version(self.version)
    self.description = args.description or rawget(_G, "script_description") or ""
    self.author = args.author or ""
    self.url = args.url
    self.feed = args.feed
    self.configFile = args.configFile
    self.requiredModules = {}
    for i, m in ipairs(args) do self.requiredModules[#self.requiredModules + 1] = m end
    self.logger = mk_logger(self.name)
    if self.moduleName == nil and rawget(_G, "script_name") == nil then rawset(_G, "script_name", self.name) end
    if rawget(_G, "script_description") == nil then rawset(_G, "script_description", self.description) end
    if rawget(_G, "script_author") == nil then rawset(_G, "script_author", self.author) end
    if rawget(_G, "script_version") == nil then rawset(_G, "script_version", tostring(self.version)) end
    return self
  end
})

function DC:getVersionString() return tostring(self.version) end
function DC:getVersionNumber() return self.versionNumber end
function DC:checkVersion(v) return self.versionNumber >= parse_version(v) end
function DC:getLogger() return self.logger end

function DC:requireModules(mods)
  mods = mods or self.requiredModules
  local out = {}
  for i, m in ipairs(mods) do
    local name, optional, ver = m, false, nil
    if type(m) == "table" then
      name = m[1] or m.moduleName
      optional = m.optional
    end
    local ok, mod = pcall(require, name)
    if not ok then
      if optional then
        out[i] = nil
      else
        error("Required module '" .. tostring(name) .. "' could not be loaded:\n" .. tostring(mod), 2)
      end
    else
      out[i] = mod
    end
  end
  return unpack(out, 1, #mods)
end

function DC:requireModulesNoUpdate(...) return self:requireModules(...) end
function DC:load(...) return self:requireModules(...) end

function DC:register(module)
  if module ~= nil and self.moduleName then
    package.loaded[self.moduleName] = module
    if type(module) == "table" and module.version == nil then module.version = self end
  end
  return module
end

function DC:registerMacro(name, desc, fn, validate, active, submenu)
  if type(name) == "function" then
    fn, validate, active, submenu = name, desc, fn, validate
    name, desc = self.name, self.description
  end
  if submenu then name = tostring(submenu) .. "/" .. name end
  aegisub.register_macro(name, desc or "", fn, validate, active)
end

function DC:registerMacros(macros, submenu)
  for _, m in ipairs(macros) do
    local name = m[1] or m.name
    local desc = m[2] or m.description or ""
    local fn = m[3] or m.fn
    local v = m[4] or m.validate
    local a = m[5] or m.active
    if submenu then name = tostring(submenu) .. "/" .. name end
    aegisub.register_macro(name, desc, fn, v, a)
  end
end

function DC:registerFilter(name, desc, prio, fn, cfg)
  aegisub.register_filter(name, desc, prio, fn, cfg)
end

DC.version = setmetatable({}, { __call = function(_, v) return v end })
function DC.parseVersion(v) return parse_version(v) end
function DC.getVersionNumber(v) return parse_version(v) end
function DC.getConfigPath() return aegisub.decode_path("?user") end
DC.updater = { update = function() end }

return DC
