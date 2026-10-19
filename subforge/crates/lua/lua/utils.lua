-- SubForge: Automation 4 compatible utils (API-compatible with Aegisub's utils.lua / aegisub.util)
local util = {}

local function lua_floor(x) return math.floor(x) end

function util.copy(t)
	local r = {}
	for k, v in pairs(t) do r[k] = v end
	return r
end

function util.deep_copy(t, seen)
	if type(t) ~= "table" then return t end
	seen = seen or {}
	if seen[t] then return seen[t] end
	local r = {}
	seen[t] = r
	for k, v in pairs(t) do r[util.deep_copy(k, seen)] = util.deep_copy(v, seen) end
	return setmetatable(r, getmetatable(t))
end

function util.clamp(v, lo, hi) if v < lo then return lo elseif v > hi then return hi end return v end

function util.trim(s) return (s:gsub("^%s+", ""):gsub("%s+$", "")) end

function util.headtail(s)
	local a, b, head, tail = s:find("(.-)%s+(.*)")
	if a then return head, tail end
	return s, ""
end

function util.words(s)
	return coroutine.wrap(function()
		for w in s:gmatch("%S+") do coroutine.yield(w) end
	end)
end

function util.ass_alpha(a) return string.format("&H%02X&", a) end
function util.ass_color(r, g, b) return string.format("&H%02X%02X%02X&", b, g, r) end
function util.ass_style_color(r, g, b, a) return string.format("&H%02X%02X%02X%02X", a or 0, b, g, r) end

function util.extract_color(s)
	local a, b, g, r
	-- &HAABBGGRR
	a, b, g, r = s:match("&H(%x%x)(%x%x)(%x%x)(%x%x)")
	if a then return tonumber(r, 16), tonumber(g, 16), tonumber(b, 16), tonumber(a, 16) end
	b, g, r = s:match("&H(%x%x)(%x%x)(%x%x)&?")
	if b then return tonumber(r, 16), tonumber(g, 16), tonumber(b, 16), 0 end
	a = s:match("^&H(%x%x)&$")
	if a then return 0, 0, 0, tonumber(a, 16) end
	r, g, b = s:match("#(%x%x)(%x%x)(%x%x)")
	if r then return tonumber(r, 16), tonumber(g, 16), tonumber(b, 16), 0 end
	local n = tonumber(s)
	if n then
		return n % 256, math.floor(n / 256) % 256, math.floor(n / 65536) % 256, 0
	end
	return 0, 0, 0, 0
end

function util.alpha_from_style(scolor)
	local r, g, b, a = util.extract_color(scolor)
	return util.ass_alpha(a)
end

function util.color_from_style(scolor)
	local r, g, b, a = util.extract_color(scolor)
	return util.ass_color(r, g, b)
end

function util.interpolate(pct, min, max)
	if pct <= 0 then return min elseif pct >= 1 then return max end
	return min + (max - min) * pct
end

function util.interpolate_color(pct, first, last)
	local r1, g1, b1 = util.extract_color(first)
	local r2, g2, b2 = util.extract_color(last)
	return util.ass_color(
		math.floor(util.interpolate(pct, r1, r2) + 0.5),
		math.floor(util.interpolate(pct, g1, g2) + 0.5),
		math.floor(util.interpolate(pct, b1, b2) + 0.5))
end

function util.interpolate_alpha(pct, first, last)
	local _, _, _, a1 = util.extract_color(first)
	local _, _, _, a2 = util.extract_color(last)
	return util.ass_alpha(math.floor(util.interpolate(pct, a1, a2) + 0.5))
end

function util.uuid()
	local t = {}
	for i = 1, 16 do t[i] = math.random(0, 255) end
	t[7] = t[7] % 16 + 64
	t[9] = t[9] % 64 + 128
	local s = string.format(string.rep("%02x", 16), unpack(t))
	return s:sub(1, 8) .. "-" .. s:sub(9, 12) .. "-" .. s:sub(13, 16) .. "-" .. s:sub(17, 20) .. "-" .. s:sub(21)
end

function util.fix_fnt(s) return s end

-- ---------------------------------------------------------------- globals
-- Old-style utils.lua exposed these as globals; keep them for legacy scripts.
function string.trim(s) return util.trim(s) end
function string.headtail(s) return util.headtail(s) end
function string.words(s) return util.words(s) end
function math.round(num, idp)
	local mult = 10 ^ (idp or 0)
	return math.floor(num * mult + 0.5) / mult
end
function table.copy(t) return util.copy(t) end
function table.copy_deep(t) return util.deep_copy(t) end
function table.join(a, b)
	local r = {}
	for _, v in ipairs(a) do r[#r + 1] = v end
	for _, v in ipairs(b) do r[#r + 1] = v end
	return r
end
function table.indexof(t, x)
	for i, v in ipairs(t) do if v == x then return i end end
	return nil
end

clamp = util.clamp
interpolate = util.interpolate
interpolate_color = util.interpolate_color
interpolate_alpha = util.interpolate_alpha
ass_alpha = util.ass_alpha
ass_color = util.ass_color
ass_style_color = util.ass_style_color
extract_color = util.extract_color
alpha_from_style = util.alpha_from_style
color_from_style = util.color_from_style
_G.util = util

return util
