-- osisub: l0.DependencyControl uyumluluk katmanı.
-- Gerçek DependencyControl ağdan güncelleme yapar ve Windows'a özel DLL'ler (BadMutex,
-- PreciseTimer, DownloadManager) ister; osisub bunun yerine aynı betik API'sini sunar:
--   local DepCtrl = require "l0.DependencyControl"
--   local dep = DepCtrl{ feed = "...", { {"a-mo.LineCollection", version = "1.3.0"}, "json" } }
--   local LineCollection, json = dep:requireModules()
--   dep:registerMacro(islev)   /  dep:registerMacros{ {"Ad", "Açıklama", islev}, ... }
--   local cfg = dep:getConfigHandler({ ayar = 1 }) ; cfg.c.ayar ; cfg:write()
-- Modüller kullanıcının include klasörlerinden (automation/include) `require` ile yüklenir;
-- güncelleme/indirme yapılmaz. Eksik modül, adı ve (varsa) adresiyle açıkça bildirilir.
local json_ok, json = pcall(require, "json")

local DependencyControl = { _osisub = true, version_string = "0.6.4-osisub" }
DependencyControl.__index = DependencyControl

-- ---- sürüm yardımcıları (DepCtrl: tamsayı = major*65536 + minor*256 + patch)
local function parse_version(v)
  if v == nil then return 0 end
  if type(v) == "number" then return math.floor(v) end
  if type(v) == "table" and v.version then return parse_version(v.version) end
  local s = tostring(v)
  local a, b, c = s:match("^v?(%d+)%.?(%d*)%.?(%d*)")
  if not a then return 0 end
  return tonumber(a) * 65536 + (tonumber(b) or 0) * 256 + (tonumber(c) or 0)
end
local function version_string(n)
  n = parse_version(n)
  return string.format("%d.%d.%d", math.floor(n / 65536), math.floor(n / 256) % 256, n % 256)
end
DependencyControl.parseVersion = function(self_or_v, v)
  if v == nil and getmetatable(self_or_v) ~= DependencyControl then return parse_version(self_or_v) end
  return parse_version(v)
end
DependencyControl.getVersionString = function(self, v)
  if v == nil and type(self) == "table" then return version_string(self.version) end
  return version_string(v or self)
end
DependencyControl.getVersionNumber = function(self, v)
  if v == nil and type(self) == "table" then return self.version end
  return parse_version(v or self)
end

-- ---- günlükçü (DepCtrl Logger)
local Logger = {}
Logger.__index = Logger
local function make_logger(prefix)
  return setmetatable({ prefix = prefix and (prefix .. ": ") or "", defaultLevel = 2 }, Logger)
end
local function fmt(msg, ...)
  msg = tostring(msg == nil and "" or msg)
  if select("#", ...) > 0 then
    local ok, r = pcall(string.format, msg, ...)
    if ok then return r end
  end
  return msg
end
function Logger:log(level, msg, ...)
  if type(level) ~= "number" then return self:log(self.defaultLevel or 3, level, msg, ...) end
  aegisub.log(level, "%s", self.prefix .. fmt(msg, ...) .. "\n")
end
function Logger:fatal(msg, ...) self:log(0, msg, ...) end
function Logger:error(msg, ...)
  local s = self.prefix .. fmt(msg, ...)
  error(s, 2)
end
function Logger:warn(msg, ...) self:log(2, msg, ...) end
function Logger:hint(msg, ...) self:log(3, msg, ...) end
function Logger:debug(msg, ...) self:log(4, msg, ...) end
function Logger:trace(msg, ...) self:log(5, msg, ...) end
function Logger:dump(v, name) self:log(4, "%s%s", name and (name .. " = ") or "", json_ok and pcall(json.encode, v) and json.encode(v) or tostring(v)) end
function Logger:assert(cond, msg, ...)
  if not cond then self:error(msg or "doğrulama başarısız", ...) end
  return cond
end
function Logger:format(msg, ...) return fmt(msg, ...) end
Logger.new = function(_, o) local l = make_logger(o and o.prefix); return l end
DependencyControl.Logger = setmetatable({}, { __index = Logger, __call = function(_, o) return make_logger(o and (o.prefix or o.fileBaseName)) end })

-- ---- yapılandırma (DepCtrl ConfigHandler): ?user/config/<namespace>.json
local ConfigHandler = {}
ConfigHandler.__index = ConfigHandler
local function deep_copy(t)
  if type(t) ~= "table" then return t end
  local r = {}
  for k, v in pairs(t) do r[k] = deep_copy(v) end
  return r
end
local function config_path(namespace)
  local dir = aegisub.decode_path("?user/config")
  local lfs = package.loaded.lfs or require("lfs")
  lfs.mkdir(aegisub.decode_path("?user"))
  lfs.mkdir(dir)
  return dir .. "/" .. namespace .. ".json"
end
local function read_file(p)
  local f = io.open(p, "rb")
  if not f then return nil end
  local s = f:read("*a")
  f:close()
  return s
end
local function make_config(path, defaults, section, hive)
  local self = setmetatable({ file = path, defaults = deep_copy(defaults or {}), section = section, hive = hive or "config" }, ConfigHandler)
  self.c = deep_copy(self.defaults)
  self.config = self.c
  self.userConfig = self.c
  self:load()
  return self
end
function ConfigHandler:_root()
  local s = read_file(self.file)
  local ok, data = false, nil
  if s and s ~= "" and json_ok then ok, data = pcall(json.decode, s) end
  if not ok or type(data) ~= "table" then data = {} end
  return data
end
function ConfigHandler:load()
  local data = self:_root()
  local node = data[self.hive]
  if type(node) ~= "table" then node = {} end
  if type(self.section) == "table" then
    for _, k in ipairs(self.section) do node = type(node) == "table" and node[k] or nil end
  elseif self.section then
    node = node[self.section]
  end
  for k, v in pairs(self.c) do self.c[k] = nil end
  for k, v in pairs(deep_copy(self.defaults)) do self.c[k] = v end
  if type(node) == "table" then
    for k, v in pairs(node) do self.c[k] = deep_copy(v) end
  end
  return true
end
function ConfigHandler:write()
  if not json_ok then return false, "json modülü yok" end
  local data = self:_root()
  data[self.hive] = type(data[self.hive]) == "table" and data[self.hive] or {}
  local parent, key = data, self.hive
  local path = type(self.section) == "table" and self.section or (self.section and { self.section } or {})
  for _, k in ipairs(path) do
    parent[key] = type(parent[key]) == "table" and parent[key] or {}
    parent, key = parent[key], k
  end
  parent[key] = deep_copy(self.c)
  local f, err = io.open(self.file, "wb")
  if not f then return false, err end
  f:write(json.encode(data))
  f:close()
  return true
end
ConfigHandler.save = ConfigHandler.write
function ConfigHandler:getSectionHandler(section, defaults)
  local sec = type(self.section) == "table" and deep_copy(self.section) or (self.section and { self.section } or {})
  if type(section) == "table" then for _, s in ipairs(section) do sec[#sec + 1] = s end else sec[#sec + 1] = section end
  return make_config(self.file, defaults, sec, self.hive)
end
function ConfigHandler:import(tbl, keys, update)
  for k, v in pairs(tbl or {}) do
    if not keys or keys[k] then self.c[k] = deep_copy(v) end
  end
  if update then self:write() end
end
function ConfigHandler:delete() for k in pairs(self.c) do self.c[k] = nil end; return self:write() end
DependencyControl.ConfigHandler = ConfigHandler

-- ---- kayıt (record) oluşturma
-- Çağıran betiğin ortamı (osisub her betiğe ayrı ortam verir): çağrı yığınında _G
-- olmayan ilk ortam; modüller _G'de çalışır.
local function caller_env(level)
  for l = level + 1, level + 8 do
    local ok, env = pcall(getfenv, l)
    if not ok then break end
    if type(env) == "table" and env ~= _G then return env end
  end
  return _G
end

local function normalize_dep(d)
  if type(d) == "string" then return { moduleName = d, name = d } end
  local r = {}
  for k, v in pairs(d) do r[k] = v end
  r.moduleName = r.moduleName or r[1]
  r.name = r.name or r.moduleName
  return r
end

local function new_record(args, env)
  args = args or {}
  local self = setmetatable({}, DependencyControl)
  self.moduleName = args.moduleName
  self.virtual = args.virtual
  self.scriptType = self.moduleName and 2 or 1 -- DepCtrl: 1 = makro betiği, 2 = modül
  self.name = args.name or (not self.moduleName and rawget(env, "script_name")) or self.moduleName or "?"
  self.description = args.description or (not self.moduleName and rawget(env, "script_description")) or ""
  self.author = args.author or (not self.moduleName and rawget(env, "script_author")) or ""
  self.namespace = args.namespace or self.moduleName or rawget(env, "script_namespace") or self.name
  self.version = parse_version(args.version or (not self.moduleName and rawget(env, "script_version")))
  self.url, self.feed = args.url, args.feed
  self.configFile = args.configFile or self.namespace
  self.requiredModules = {}
  local deps = args[1] or args.requiredModules or {}
  for i, d in ipairs(deps) do self.requiredModules[i] = normalize_dep(d) end
  self.logger = make_logger(self.name)
  if not self.moduleName then
    -- DepCtrl betik ortamında script_* değişkenlerini doldurur (script_version dize olarak).
    if rawget(env, "script_name") == nil then env.script_name = self.name end
    if rawget(env, "script_description") == nil then env.script_description = self.description end
    if rawget(env, "script_author") == nil then env.script_author = self.author end
    if rawget(env, "script_version") == nil then env.script_version = version_string(self.version) end
    if rawget(env, "script_namespace") == nil then env.script_namespace = self.namespace end
  end
  return self
end

setmetatable(DependencyControl, {
  __call = function(_, args) return new_record(args, caller_env(2)) end,
})
DependencyControl.new = function(_, args) return new_record(args, caller_env(2)) end

local function module_version(mod)
  if type(mod) ~= "table" then return nil end
  local v = rawget(mod, "version")
  if type(v) == "table" and getmetatable(v) == DependencyControl then return v.version end
  if v ~= nil and (type(v) == "number" or type(v) == "string") then return parse_version(v) end
  return nil
end

function DependencyControl:requireModules(modules)
  modules = modules or self.requiredModules
  local out, n, missing = {}, 0, {}
  for i, d in ipairs(modules) do
    d = normalize_dep(d)
    local ok, mod = pcall(require, d.moduleName)
    if not ok then
      if not d.optional then
        missing[#missing + 1] = string.format("  - %s%s%s\n    (%s)", d.moduleName,
          d.version and (" ≥ " .. version_string(d.version)) or "",
          d.url and (" — " .. d.url) or "", tostring(mod):gsub("\n.*", ""))
      end
      mod = nil
    elseif d.version then
      local have = module_version(mod)
      if have and have < parse_version(d.version) then
        self.logger:warn("%s modülünün %s sürümü gerekli, yüklü olan %s (osisub güncelleme yapmaz)",
          d.moduleName, version_string(d.version), version_string(have))
      end
    end
    out[i] = mod
    n = i
  end
  if #missing > 0 then
    error(string.format("%s: gerekli modüller bulunamadı (automation/include klasörüne kurun):\n%s",
      self.name, table.concat(missing, "\n")), 2)
  end
  return unpack(out, 1, n)
end

function DependencyControl:checkVersion(v, precision)
  local want = parse_version(v)
  return self.version >= want, self.version
end
function DependencyControl:checkOptionalModules(modules)
  local res = {}
  for _, m in ipairs(type(modules) == "table" and modules or { modules }) do res[#res + 1] = (pcall(require, m)) end
  return unpack(res)
end

-- Modül kaydı: `return version:register(Modul)`
function DependencyControl:register(selfRef, ...)
  if type(selfRef) == "table" and rawget(selfRef, "version") == nil then
    pcall(rawset, selfRef, "version", self)
  end
  self.ref = selfRef
  return selfRef, ...
end

local function menu_name(self, name, submenu)
  if submenu == false or submenu == nil then return name end
  local group = type(submenu) == "string" and submenu or self.name
  return group .. "/" .. name
end

-- registerMacro(islev, dogrula?, etkin?, altmenu?) ya da registerMacro(ad, açıklama, islev, dogrula?, etkin?, altmenu?)
function DependencyControl:registerMacro(name, description, process, validate, isActive, submenu)
  if type(name) == "function" then
    name, description, process, validate, isActive, submenu = self.name, self.description, name, description, process, validate
  end
  aegisub.register_macro(menu_name(self, name, submenu), description or "", process, validate, isActive)
end

function DependencyControl:registerMacros(macros, submenuDefault)
  if submenuDefault == nil then submenuDefault = true end
  for _, m in ipairs(macros) do
    local name, desc, process, validate, isActive, submenu = m[1] or m.name, m[2] or m.description, m[3] or m.process, m[4] or m.validate, m[5] or m.isActive, m[6]
    if submenu == nil then submenu = submenuDefault end
    aegisub.register_macro(menu_name(self, name, submenu), desc or "", process, validate, isActive)
  end
end

function DependencyControl:registerTests() end
function DependencyControl:getConfigHandler(defaults, section, noLoad)
  return make_config(config_path(self.configFile), defaults, section)
end
function DependencyControl:getConfigFileName() return config_path(self.configFile) end
function DependencyControl:getLogger(args) return make_logger((args and args.prefix) or self.name) end
function DependencyControl:loadConfig() return true end
function DependencyControl:writeConfig() return true end
function DependencyControl:update() return 0, "osisub: DependencyControl güncellemeleri devre dışı" end
function DependencyControl:getSubmodules() return {} end
function DependencyControl:uninstall() return false, "osisub: desteklenmiyor" end

-- Seyrek kullanılan yardımcı sınıflar için zararsız yer tutucular
DependencyControl.UpdateFeed = setmetatable({}, { __call = function() return setmetatable({}, { __index = function() return function() return nil end end }) end })
DependencyControl.UnitTestSuite = setmetatable({}, { __call = function() return { run = function() return true end, import = function() end } end })
DependencyControl.FileOps = {
  copy = function(src, dst)
    local s = read_file(src)
    if not s then return false, "okunamadı: " .. tostring(src) end
    local f = io.open(dst, "wb"); if not f then return false end
    f:write(s); f:close(); return true
  end,
  remove = function(p) return os.remove(p) end,
  mkdir = function(p) local lfs = require("lfs"); return lfs.mkdir(p) end,
}

package.loaded["l0.DependencyControl.Logger"] = DependencyControl.Logger
package.loaded["l0.DependencyControl.ConfigHandler"] = ConfigHandler
package.loaded["l0.DependencyControl.UpdateFeed"] = DependencyControl.UpdateFeed
package.loaded["l0.DependencyControl.UnitTestSuite"] = DependencyControl.UnitTestSuite
package.loaded["l0.DependencyControl.FileOps"] = DependencyControl.FileOps

return DependencyControl
