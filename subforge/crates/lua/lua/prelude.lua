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

aegisub.set_undo_point = function(name) end
aegisub.cancel_requested = nil

-- `ipairs(subs)` must work on the userdata subtitle object (Aegisub supports it).
do
	local raw_ipairs = ipairs
	function ipairs(t)
		if type(t) == "userdata" then
			return function(o, i)
				i = i + 1
				if i <= #o then return i, o[i] end
			end, t, 0
		end
		return raw_ipairs(t)
	end
end

-- aegisub.parse_karaoke_data(line): syllables of a dialogue line, indexed from 0
-- (0 = text before the first \k). Times are relative to line start, in ms.
function aegisub.parse_karaoke_data(line)
	local text = line.text or ""
	local syls = {}
	local cur
	local elapsed = 0
	local function new_syl(dur, tag)
		cur = { raw = {}, stripped = {}, open = {}, duration = dur or 0, tag = tag or "k", start_time = elapsed }
	end
	local function flush_block()
		if #cur.open > 0 then
			cur.raw[#cur.raw + 1] = "{" .. table.concat(cur.open) .. "}"
			cur.open = {}
		end
	end
	local function finish()
		flush_block()
		local s = cur
		s.text = table.concat(s.raw)
		s.text_stripped = table.concat(s.stripped)
		s.end_time = s.start_time + s.duration
		s.raw, s.stripped, s.open = nil, nil, nil
		syls[#syls + 1] = s
		elapsed = s.end_time
	end
	new_syl(0, "k")
	local pos = 1
	while pos <= #text do
		local bs, be = text:find("{[^}]*}", pos)
		if bs == pos then
			local block = text:sub(bs + 1, be - 1)
			-- split into pieces at top-level backslashes
			local pieces, start, depth = {}, 1, 0
			for i = 1, #block do
				local c = block:sub(i, i)
				if c == "(" then depth = depth + 1
				elseif c == ")" then depth = depth - 1
				elseif c == "\\" and depth <= 0 and i > start then
					pieces[#pieces + 1] = block:sub(start, i - 1)
					start = i
				end
			end
			pieces[#pieces + 1] = block:sub(start)
			for _, p in ipairs(pieces) do
				local ktag, kval = p:match("^\\([kK][fo]?)%s*(%-?[%d%.]+)%s*$")
				if ktag then
					finish()
					if ktag == "K" then ktag = "kf" end
					new_syl(math.floor(tonumber(kval) * 10 + 0.5), ktag)
				elseif p ~= "" then
					cur.open[#cur.open + 1] = p
				end
			end
			pos = be + 1
		else
			local stop = bs and (bs - 1) or #text
			flush_block()
			local chunk = text:sub(pos, stop)
			cur.raw[#cur.raw + 1] = chunk
			cur.stripped[#cur.stripped + 1] = chunk
			pos = stop + 1
		end
	end
	finish()
	local out = {}
	for i, s in ipairs(syls) do out[i - 1] = s end
	-- zero-length trailing empty syllable (e.g. text ends with a \k tag) is kept, like Aegisub
	return out
end
