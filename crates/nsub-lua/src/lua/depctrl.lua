-- Minimal DependencyControl (l0.DependencyControl) shim so scripts written for it load without it.
-- It never downloads anything; it only resolves `requireModules` with plain `require`.
local DC = {}
DC.__index = DC
DC.version = "0.6.3-nightsub"

local function new(_, record)
  record = record or {}
  local self = setmetatable({}, DC)
  self.scriptName = record.name or record.scriptName or script_name or "?"
  self.name = self.scriptName
  self.version = record.version or "0.0.0"
  self.moduleName = record.moduleName
  self.description = record.description
  self.author = record.author
  self.namespace = record.namespace or record.moduleName or self.scriptName
  self.feed = record.url and record.feed
  self.configFile = record.configFile
  self.requiredModules = record.feed and {} or {}
  self.requireModules = nil
  self.depRecord = record
  return self
end

local function modname(m)
  if type(m) == "table" then return m[1] or m.moduleName, m end
  return m, {}
end

function DC:requireModules(modules)
  modules = modules or self.depRecord
  modules = modules.moduleName and modules or modules
  local list = modules
  if type(list) == "table" and list[1] == nil then list = self.depRecord end
  if type(list) == "table" and list.moduleName == nil and list[1] == nil then return end
  local out = {}
  local names = {}
  for i = 1, #list do names[i] = list[i] end
  for i, m in ipairs(names) do
    local name, spec = modname(m)
    local ok, mod = pcall(require, name)
    if not ok then
      if spec.optional then mod = nil else error("DependencyControl shim: required module '" .. tostring(name) .. "' not found:\n" .. tostring(mod), 2) end
    end
    out[#out + 1] = mod
  end
  return table.unpack and table.unpack(out) or unpack(out)
end
DC.requireModules = DC.requireModules

function DC:registerMacro(name, desc, fn, validate, active)
  if type(name) == "function" then fn, validate, active = name, desc, fn; name, desc = self.name, self.description end
  aegisub.register_macro(name, desc or "", fn, validate, active)
end
function DC:registerMacros(macros)
  for _, m in ipairs(macros) do
    local name, desc, fn, validate, active = m[1] or m.name, m[2] or m.description, m[3] or m.fn, m[4] or m.validate, m[5] or m.active
    aegisub.register_macro(self.name .. "/" .. name, desc or "", fn, validate, active)
  end
end
function DC:registerFilter(...) aegisub.register_filter(...) end
function DC:register(...) return self end
function DC:getVersion() return self.version end
function DC:checkVersion() return true end
function DC:checkOptionalModules() return true end
function DC:getConfigFileName() return self.configFile or (self.name .. ".json") end
function DC:getLogger()
  local l = {}
  function l:log(...) aegisub.log(...) end
  function l:trace(...) end
  l.logger = l
  return l
end
DC.logger = DC:getLogger()

return setmetatable(DC, { __call = new })
