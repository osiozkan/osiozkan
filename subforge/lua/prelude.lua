-- SubForge Automation 4 compatibility prelude.
-- `__sf` is a native table provided by the host: log, text_extents, re_*, params, fps, script_dir, etc.

local sf = __sf
aegisub = aegisub or {}
local aegisub = aegisub

sf.macros = {}
sf.filters = {}
sf.export_filters = sf.filters

aegisub.lua_supported_version = 3
aegisub.__version = "3.2.2-subforge"
aegisub.progress = {
	set = function(v) sf.progress("set", v) end,
	task = function(t) sf.progress("task", t) end,
	title = function(t) sf.progress("title", t) end,
	is_cancelled = function() return sf.is_cancelled() end,
}
aegisub.gettext = function(s) return s end
aegisub.cancel = function() error("__sf_cancel__", 0) end

local function tostr(v) if v == nil then return "nil" end return tostring(v) end

aegisub.log = function(a, b, ...)
	local level, msg
	if type(a) == "number" then level, msg = a, b; if select("#", ...) > 0 then msg = string.format(msg, ...) end
	else
		msg = a
		if b ~= nil then msg = string.format(a, b, ...) end
	end
	sf.log(level or 2, tostr(msg))
end
aegisub.debug = {
	out = function(a, b, ...)
		if type(a) == "number" then
			if a >= 4 then return end -- trace levels hidden by default, like Aegisub
			return aegisub.log(a, b, ...)
		end
		return aegisub.log(a, b, ...)
	end,
}
aegisub.parse_karaoke_data = function(line) return sf.parse_karaoke_data(line.text) end

function aegisub.register_macro(name, description, process, validate, is_active)
	table.insert(sf.macros, { name = name, description = description, process = process, validate = validate, is_active = is_active })
end
function aegisub.register_filter(name, description, priority, process, config)
	table.insert(sf.filters, { name = name, description = description, priority = priority, process = process, config = config })
end

aegisub.text_extents = function(style, text) return sf.text_extents(style, text) end
aegisub.frame_from_ms = function(ms) if not sf.fps then return nil end return math.floor(ms * sf.fps / 1000) end
aegisub.ms_from_frame = function(f) if not sf.fps then return nil end return math.floor(f * 1000 / sf.fps + 0.5) end
aegisub.video_size = function() return nil end
aegisub.keyframes = function() return {} end
aegisub.project_properties = function()
	local p = sf.project_properties()
	return p
end
aegisub.decode_path = function(p) return sf.decode_path(p) end
aegisub.get_audio_selection = function() return nil end
aegisub.get_cursor = function() return nil end
aegisub.set_undo_point = function() end

-- ---- dialogs (headless: return defaults, overridden by --param name=value) ----
local function dialog_defaults(dialog)
	local res = {}
	for _, c in ipairs(dialog) do
		if c.name and c.class ~= "label" then
			local v
			if c.class == "edit" or c.class == "textbox" then v = c.text or ""
			elseif c.class == "intedit" or c.class == "floatedit" then v = c.value or 0
			elseif c.class == "checkbox" then v = c.value or false
			elseif c.class == "dropdown" then v = c.value or (c.items and c.items[1]) or ""
			elseif c.class == "coloralpha" or c.class == "color" then v = c.value or "&H00FFFFFF&"
			elseif c.class == "slider" then v = c.value or c.min or 0
			elseif c.class == "alpha" then v = c.value or "&H00&"
			else v = c.value or c.text end
			local ov = sf.params[c.name]
			if ov ~= nil then
				if type(v) == "number" then ov = tonumber(ov) or v
				elseif type(v) == "boolean" then ov = (ov == "true" or ov == "1" or ov == "yes")
				end
				v = ov
			end
			res[c.name] = v
		end
	end
	return res
end
aegisub.dialog = {
	display = function(dialog, buttons, button_ids)
		if sf.has_dialog then
			local btn, vals = sf.dialog(dialog, buttons, button_ids)
			if btn == nil or btn == false then return false, {} end
			return btn, vals
		end
		local res = dialog_defaults(dialog)
		if buttons == nil then return "Ok", res end
		local ok = (button_ids and button_ids.ok) or buttons[1]
		sf.log(3, "[dialog] auto-confirmed with '" .. tostr(ok) .. "'")
		return ok, res
	end,
	open = function() return nil end,
	save = function() return nil end,
}
aegisub.dialog.display = (function(f)
	return function(dialog, buttons, button_ids)
		if buttons == nil then
			local b, res = f(dialog, buttons, button_ids)
			if b == false then return false, {} end
			return true, res
		end
		return f(dialog, buttons, button_ids)
	end
end)(aegisub.dialog.display)
aegisub.clipboard = { get = function() return nil end, set = function() return false end }
clipboard = aegisub.clipboard

-- ---- util ----
local util = {}
function util.clamp(v, lo, hi) if v < lo then return lo elseif v > hi then return hi else return v end end
function util.interpolate(pct, a, b) return a + (b - a) * util.clamp(pct, 0, 1) end
local function round(x) return math.floor(x + 0.5) end
function util.ass_color(r, g, b) return string.format("&H%02X%02X%02X&", util.clamp(round(b), 0, 255), util.clamp(round(g), 0, 255), util.clamp(round(r), 0, 255)) end
function util.ass_alpha(a) return string.format("&H%02X&", util.clamp(round(a), 0, 255)) end
function util.ass_style_color(r, g, b, a) return string.format("&H%02X%02X%02X%02X", util.clamp(round(a or 0), 0, 255), util.clamp(round(b), 0, 255), util.clamp(round(g), 0, 255), util.clamp(round(r), 0, 255)) end
function util.extract_color(s)
	local a, b, g, r = 0, 0, 0, 0
	local m = { s:match("^&H(%x%x)(%x%x)(%x%x)(%x%x)") }
	if #m == 4 then a, b, g, r = tonumber(m[1], 16), tonumber(m[2], 16), tonumber(m[3], 16), tonumber(m[4], 16)
	else
		local m3 = { s:match("^&H(%x%x)(%x%x)(%x%x)&?") }
		if #m3 == 3 then b, g, r = tonumber(m3[1], 16), tonumber(m3[2], 16), tonumber(m3[3], 16)
		else
			local m1 = s:match("^&H(%x%x)&?$")
			if m1 then a = tonumber(m1, 16) else
				local n = tonumber(s)
				if n then r = n % 256; g = math.floor(n / 256) % 256; b = math.floor(n / 65536) % 256; a = math.floor(n / 16777216) % 256 end
			end
		end
	end
	return r, g, b, a
end
function util.alpha_from_style(s) local _, _, _, a = util.extract_color(s); return util.ass_alpha(a) end
function util.color_from_style(s) local r, g, b = util.extract_color(s); return util.ass_color(r, g, b) end
function util.interpolate_color(pct, c1, c2)
	local r1, g1, b1 = util.extract_color(c1); local r2, g2, b2 = util.extract_color(c2)
	return util.ass_color(util.interpolate(pct, r1, r2), util.interpolate(pct, g1, g2), util.interpolate(pct, b1, b2))
end
function util.interpolate_alpha(pct, a1, a2)
	local _, _, _, x = util.extract_color(a1); local _, _, _, y = util.extract_color(a2)
	return util.ass_alpha(util.interpolate(pct, x, y))
end
function util.trim(s) return (s:gsub("^%s*(.-)%s*$", "%1")) end
function util.headtail(s)
	local a, b = s:match("^(%S*)%s*(.*)$")
	return a, b
end
function util.words(s) local t = {}; for w in s:gmatch("%S+") do t[#t + 1] = w end; return t end
function util.copy(t) local n = {}; for k, v in pairs(t) do n[k] = v end; return n end
function util.deep_copy(t)
	local seen = {}
	local function dc(x)
		if type(x) ~= "table" then return x end
		if seen[x] then return seen[x] end
		local n = {}; seen[x] = n
		for k, v in pairs(x) do n[dc(k)] = dc(v) end
		return setmetatable(n, getmetatable(x))
	end
	return dc(t)
end
function util.split(s, sep) local t = {}; for part in (s .. sep):gmatch("(.-)" .. sep:gsub("%p", "%%%0")) do t[#t + 1] = part end; return t end
aegisub.util = util

-- legacy utils.lua globals
function string.trim(s) return util.trim(s) end
function string.headtail(s) return util.headtail(s) end
function string.words(s) return util.words(s) end
function table.copy(t) return util.copy(t) end
function table.copy_deep(t) return util.deep_copy(t) end
function math.round(x) return math.floor(x + 0.5) end
ass_color, ass_alpha, ass_style_color = util.ass_color, util.ass_alpha, util.ass_style_color
extract_color, alpha_from_style, color_from_style = util.extract_color, util.alpha_from_style, util.color_from_style
interpolate_color, interpolate_alpha, interpolate = util.interpolate_color, util.interpolate_alpha, util.interpolate
clamp = util.clamp
function string.split(s, sep) return util.split(s, sep) end
function table.append(a, b) for _, v in ipairs(b) do a[#a + 1] = v end; return a end

-- ---- unicode ----
local unicode = {}
local function chars(s) local t = {}; for c in s:gmatch("[%z\1-\127\194-\244][\128-\191]*") do t[#t + 1] = c end; return t end
function unicode.chars(s) local i, t = 0, chars(s); return function() i = i + 1; return t[i] end end
function unicode.len(s) return #chars(s) end
function unicode.charwidth(s, i)
	i = i or 1
	local b = s:byte(i)
	if not b then return 0 end
	if b < 0x80 then return 1 elseif b < 0xE0 then return 2 elseif b < 0xF0 then return 3 else return 4 end
end
function unicode.codepoint(c) return sf.codepoint(c) end
function unicode.codepoints(s) local i, t = 0, chars(s); return function() i = i + 1; if t[i] then return sf.codepoint(t[i]) end end end
function unicode.sub(s, i, j) local t = chars(s); return table.concat(t, "", i, j or #t) end
function unicode.to_upper(s) return sf.upper(s) end
function unicode.to_lower(s) return sf.lower(s) end
function unicode.to_fold_case(s) return sf.lower(s) end
unicode.to_upper_case, unicode.to_lower_case = unicode.to_upper, unicode.to_lower
aegisub.unicode = unicode
_G.unicode = unicode

-- ---- re (Boost-like regex via fancy-regex) ----
local re = { ICASE = 1, NOSUB = 2, NO_MOD_M = 4, MOD_S = 8, MOD_X = 16, NEWLINE_ANY = 0, NEWLINE_CR = 0, NEWLINE_LF = 0 }
function re.find(s, p, flags) return sf.re_find(s, p, flags or 0) end
function re.match(s, p, flags) return sf.re_match(s, p, flags or 0) end
function re.sub(s, p, r, count, flags)
	if type(count) ~= "number" then flags, count = count, 0 end
	return sf.re_sub(s, p, r, count or 0, flags or 0)
end
re.gsub = re.sub
function re.split(s, p, flags) return sf.re_split(s, p, flags or 0) end
function re.compile(p, flags)
	local o = {}
	flags = flags or 0
	function o.find(s) return re.find(s, p, flags) end
	function o.match(s) return re.match(s, p, flags) end
	function o.sub(s, r, c) return re.sub(s, p, r, c, flags) end
	o.gsub = o.sub
	function o.split(s) return re.split(s, p, flags) end
	return o
end
aegisub.re = re

-- ---- LuaJIT-ish `bit` ----
if not bit then
	local function tobit(x) x = x % 4294967296; if x >= 2147483648 then x = x - 4294967296 end; return x end
	local function op(f) return function(a, b, ...)
		local r = f(tobit(a) % 4294967296, tobit(b) % 4294967296)
		for i = 1, select("#", ...) do r = f(r, select(i, ...) % 4294967296) end
		return tobit(r) end end
	local function bitwise(a, b, fn)
		local r, p = 0, 1
		for _ = 1, 32 do
			local x, y = a % 2, b % 2
			if fn(x, y) then r = r + p end
			a, b, p = (a - x) / 2, (b - y) / 2, p * 2
		end
		return r
	end
	bit = {
		tobit = tobit,
		band = op(function(a, b) return bitwise(a, b, function(x, y) return x == 1 and y == 1 end) end),
		bor = op(function(a, b) return bitwise(a, b, function(x, y) return x == 1 or y == 1 end) end),
		bxor = op(function(a, b) return bitwise(a, b, function(x, y) return x ~= y end) end),
		bnot = function(a) return tobit(-tobit(a) - 1) end,
		lshift = function(a, n) return tobit((a % 4294967296) * 2 ^ (n % 32)) end,
		rshift = function(a, n) return tobit(math.floor((a % 4294967296) / 2 ^ (n % 32))) end,
		arshift = function(a, n) return tobit(math.floor(tobit(a) / 2 ^ (n % 32))) end,
	}
end

-- ---- include / require ----
local loaded_includes = {}
function include(name)
	if loaded_includes[name] then return end
	loaded_includes[name] = true
	if name == "utils.lua" or name == "unicode.lua" or name == "clipboard.lua" then return end
	local src, path = sf.find_include(name)
	if not src then error("Could not find include file '" .. tostring(name) .. "'", 2) end
	local f, err = loadstring(src, "@" .. path)
	if not f then error(err, 2) end
	f()
end

package.preload["aegisub.util"] = function() return util end
package.preload["aegisub.re"] = function() return re end
package.preload["re"] = function() return re end
package.preload["aegisub.unicode"] = function() return unicode end
package.preload["unicode"] = function() return unicode end
package.preload["aegisub.clipboard"] = function() return aegisub.clipboard end
package.preload["clipboard"] = function() return aegisub.clipboard end
package.preload["aegisub.lfs"] = function() return sf.lfs end
package.preload["lfs"] = function() return sf.lfs end
package.preload["aegisub"] = function() return aegisub end

-- Aegisub's `subs` is userdata that ipairs() can walk (LuaJIT); make PUC Lua behave the same.
do
	local _ipairs = ipairs
	function ipairs(t)
		if type(t) == "userdata" then
			return function(_, i)
				i = i + 1
				if i <= #t then return i, t[i] end
			end, t, 0
		end
		return _ipairs(t)
	end
end

-- print() goes to the macro log (Aegisub shows it in the debug console)
function print(...)
	local t = {}
	for i = 1, select("#", ...) do t[#t + 1] = tostring((select(i, ...))) end
	sf.log(2, table.concat(t, "\t") .. "\n")
end
