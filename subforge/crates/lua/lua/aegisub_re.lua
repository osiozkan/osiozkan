-- Port of Aegisub's `aegisub.re` (automation/include/aegisub/re.moon, ISC license,
-- Copyright (c) 2014 Thomas Goyne) on top of SubForge's native regex primitives.
local impl = ...

local flag_mt = { __flag = true }

local function search(re, str, start)
	if start > #str then return end
	return impl.search(re, str, start)
end

local function process_flags(...)
	local flags = 0
	for i = 1, select("#", ...) do
		local v = select(i, ...)
		if type(v) ~= "table" or not v.__flag then
			error("Flags must follow all non-flag arguments", 3)
		end
		flags = bit.bor(flags, v.value)
	end
	return flags
end

local function select_first(n, a, ...)
	if n == 0 then return end
	return a, select_first(n - 1, ...)
end

-- flags may trail the regular arguments; returns (flags, args...)
local function unpack_args(...)
	local flags_start
	for i = 1, select("#", ...) do
		local v = select(i, ...)
		if type(v) == "table" and v.__flag then
			flags_start = i
			break
		end
	end
	if not flags_start then return 0, ... end
	return process_flags(select(flags_start, ...)), select_first(flags_start - 1, ...)
end

local RegEx = {}
RegEx.__index = RegEx

local function replace_match(m, func, str, last, acc)
	if last < m.last then acc[#acc + 1] = str:sub(last, m.first - 1) end
	local repl = func(m.str, m.first, m.last)
	acc[#acc + 1] = type(repl) == "string" and repl or m.str
	return m.first, m.last + 1
end

local function do_single_replace_fun(re, func, str, acc, pos)
	local matches = re:match(str, pos)
	if not matches then return pos end
	local start = #matches == 1 and 1 or 2
	local last, first = pos, nil
	for i = start, #matches do
		first, last = replace_match(matches[i], func, str, last, acc)
	end
	if first == last then
		acc[#acc + 1] = str:sub(last, last)
		last = last + 1
	end
	return last, matches[1].first <= #str
end

local function do_replace_fun(re, func, str, max)
	local acc, pos = {}, 1
	for i = 1, max do
		local more
		pos, more = do_single_replace_fun(re, func, str, acc, pos)
		if not more then break end
	end
	return table.concat(acc, "") .. str:sub(pos)
end

function RegEx:gsplit(str, skip_empty, max_split)
	if not max_split or max_split <= 0 then max_split = #str end
	local start, prev = 0, 1
	local function do_split()
		if not str or #str == 0 then return end
		local first, last
		if max_split > 0 then first, last = search(self._regex, str, start) end
		if not first or first > #str then
			local ret = str:sub(prev, #str)
			str = nil
			if skip_empty and #ret == 0 then return nil end
			return ret
		end
		local ret = str:sub(prev, first - 1)
		prev = last + 1
		if start >= last then start = start + 1 else start = last end
		if skip_empty and #ret == 0 then return do_split() end
		max_split = max_split - 1
		return ret
	end
	return do_split
end

function RegEx:split(str, skip_empty, max_split)
	local out = {}
	for v in self:gsplit(str, skip_empty, max_split) do out[#out + 1] = v end
	return out
end

function RegEx:gfind(str)
	local start = 0
	return function()
		local first, last = search(self._regex, str, start)
		if not first then return end
		start = last > start and last or start + 1
		return str:sub(first, last), first, last
	end
end

function RegEx:find(str)
	local ret = {}
	for s, f, l in self:gfind(str) do ret[#ret + 1] = { str = s, first = f, last = l } end
	if next(ret) then return ret end
end

function RegEx:sub(str, repl, max_count)
	if not max_count or max_count == 0 then max_count = #str + 1 end
	if type(repl) == "function" then
		return do_replace_fun(self, repl, str, max_count)
	elseif type(repl) == "string" then
		return impl.replace(self._regex, repl, str, max_count)
	end
end

function RegEx:gmatch(str, start)
	start = start and start - 1 or 0
	local m = start <= #str and impl.match(self._regex, str, start) or nil
	local i = 0
	return function()
		if not m then return end
		i = i + 1
		local g = m[i]
		if not g then return end
		return { str = str:sub(g[1], g[2]), first = g[1], last = g[2] }
	end
end

function RegEx:match(str, start)
	local ret = {}
	for v in self:gmatch(str, start) do ret[#ret + 1] = v end
	if next(ret) then return ret end
end

local function real_compile(pattern, level, flags)
	if pattern == "" then error("Regular expression must not be empty", level + 1) end
	local re, err = impl.compile(pattern, flags)
	if not re then error(err, level + 1) end
	return setmetatable({ _regex = re }, RegEx)
end

local function wrap(name)
	return function(str, pattern, ...)
		if type(str) ~= "string" or type(pattern) ~= "string" then
			error("bad argument to re." .. name .. ": strings expected", 2)
		end
		local flags, a, b, c = unpack_args(...)
		local re = real_compile(pattern, 3, flags)
		return re[name](re, str, a, b, c)
	end
end

local re = {
	compile = function(pattern, ...)
		return real_compile(pattern, 2, process_flags(...))
	end,
}
for _, n in ipairs { "split", "gsplit", "find", "gfind", "match", "gmatch", "sub" } do re[n] = wrap(n) end
for name, value in pairs(impl.flags) do re[name] = setmetatable({ name = name, value = value }, flag_mt) end
for _, f in pairs(re) do
	if type(f) == "table" then f.__flag = true end
end
re.gsub = re.sub
return re
