-- SubForge Automation 4 prelude: pure-Lua parts of the `aegisub` table.
local host_log = aegisub.__log
local function fmt(msg, ...)
	if select("#", ...) > 0 then
		local ok, r = pcall(string.format, msg, ...)
		if ok then return r end
	end
	return tostring(msg)
end

aegisub.debug = {
	out = function(a, b, ...)
		if type(a) == "number" then
			host_log(a, fmt(b == nil and "" or b, ...))
		else
			host_log(3, fmt(a, b, ...))
		end
	end,
}
aegisub.log = function(a, b, ...)
	if type(a) == "number" then
		host_log(a, fmt(b == nil and "" or b, ...))
	else
		host_log(3, fmt(a, b, ...))
	end
end
aegisub.gettext = function(s) return s end
aegisub.lua_supports_utf8 = true

local function ms_to_frame(ms) return aegisub.__frame_from_ms(ms) end
aegisub.frame_from_ms = ms_to_frame
aegisub.ms_from_frame = function(f) return aegisub.__ms_from_frame(f) end

-- Dialog helpers: Aegisub passes (dialog, buttons, button_ids)
aegisub.dialog = {
	display = function(dialog, buttons, ids)
		return aegisub.__dialog(dialog, buttons, ids)
	end,
	open = function(title, dir, file, wildcard, multiple, must_exist)
		return aegisub.__open_dialog(title or "", dir or "", file or "", wildcard or "", multiple and true or false)
	end,
	save = function(title, dir, file, wildcard, overwrite_prompt)
		return aegisub.__save_dialog(title or "", dir or "", file or "", wildcard or "")
	end,
}

aegisub.progress = aegisub.progress or {}

-- Classic (non-ffi) helpers some scripts expect
aegisub.util = nil
