-- Minimal stand-in for line0's DependencyControl (`require "l0.DependencyControl"`).
-- It implements the registration/require surface scripts use; there is no updater,
-- feed handling or config persistence.
local function parse_version(v)
	if type(v) == "number" then return v end
	local a, b, c = tostring(v or "0"):match("^(%d+)%.?(%d*)%.?(%d*)")
	return (tonumber(a) or 0) * 65536 + (tonumber(b) or 0) * 256 + (tonumber(c) or 0)
end

local function version_string(n)
	return ("%d.%d.%d"):format(math.floor(n / 65536), math.floor(n / 256) % 256, n % 256)
end

local Logger = {}
Logger.__index = Logger
function Logger:log(level, msg, ...)
	if type(level) ~= "number" then level, msg = 2, level end
	if self.prefix and self.prefix ~= "" then msg = self.prefix .. tostring(msg) end
	aegisub.log(level, tostring(msg), ...)
	if msg:sub(-1) ~= "\n" then aegisub.log(level, "\n") end
end
for name, lvl in pairs { fatal = 0, error = 1, warn = 2, hint = 3, debug = 4, trace = 5 } do
	Logger[name] = function(self, msg, ...) return self:log(lvl, msg, ...) end
end
function Logger:assert(cond, msg, ...)
	if not cond then error((msg or "assertion failed"):format(...), 2) end
	return cond
end
function Logger:dump(o) self:log(4, tostring(o)) end

local DependencyControl = {}
DependencyControl.__index = DependencyControl
DependencyControl.version = parse_version("0.6.3")
DependencyControl.versionString = "0.6.3"

local function module_name(spec)
	if type(spec) == "string" then return spec end
	return spec.moduleName or spec[1]
end

setmetatable(DependencyControl, {
	__call = function(cls, args)
		args = args or {}
		local self = setmetatable({}, cls)
		self.name = args.name or rawget(_G, "script_name")
		self.description = args.description or rawget(_G, "script_description")
		self.author = args.author or rawget(_G, "script_author")
		self.url = args.url
		self.feed = args.feed
		self.moduleName = args.moduleName
		self.namespace = args.moduleName or args.namespace or rawget(_G, "script_namespace")
		self.scriptType = args.moduleName and "module" or "macro"
		self.version = parse_version(args.version or rawget(_G, "script_version"))
		self.versionString = version_string(self.version)
		self.requiredModules = {}
		for _, m in ipairs(args) do self.requiredModules[#self.requiredModules + 1] = m end
		self.logger = setmetatable({ prefix = self.name and (self.name .. ": ") or "" }, Logger)
		return self
	end,
})

function DependencyControl:getVersionString() return self.versionString end
function DependencyControl:getVersionNumber() return self.version end
function DependencyControl:checkVersion(v) return self.version >= parse_version(v) end
function DependencyControl:getLogger(args) return setmetatable({ prefix = args and args.prefix or self.logger.prefix }, Logger) end

function DependencyControl:requireModules(modules)
	modules = modules or self.requiredModules
	local out, n = {}, 0
	for i, spec in ipairs(modules) do
		local name = module_name(spec)
		local ok, mod = pcall(require, name)
		if not ok then
			if type(spec) == "table" and spec.optional then
				mod = nil
			else
				error(("Module '%s' required by %s could not be loaded: %s"):format(name, tostring(self.name), tostring(mod)), 2)
			end
		end
		if type(spec) == "table" and spec.name and mod ~= nil then _G[spec.name] = mod end
		out[i] = mod
		n = i
	end
	return unpack(out, 1, n)
end
DependencyControl.loadModules = DependencyControl.requireModules

function DependencyControl:register(mod)
	if mod then mod.version = self end
	return mod
end
DependencyControl.registerModule = DependencyControl.register

function DependencyControl:registerMacro(name, description, process, validate, is_active, submenu)
	if type(name) == "function" then
		name, description, process, validate, is_active, submenu = self.name, self.description, name, description, process, validate
	end
	if submenu then name = self.name .. "/" .. name end
	aegisub.register_macro(name or self.name, description or self.description or "", process, validate, is_active)
end

function DependencyControl:registerMacros(macros, submenu)
	for _, m in ipairs(macros) do
		local name = m.name or m[1]
		if submenu then name = self.name .. "/" .. name end
		aegisub.register_macro(name, m.description or m[2] or "", m.process or m[3], m.validate or m[4], m.isActive or m[5])
	end
end

function DependencyControl:registerTests() end
function DependencyControl:unregister() end
function DependencyControl:checkOptionalModules() return true end
function DependencyControl.parseVersion(_, v) return parse_version(v) end

DependencyControl.Logger = Logger
return DependencyControl
