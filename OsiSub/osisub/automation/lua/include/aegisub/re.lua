-- aegisub.re — düzenli ifadeler (Aegisub re.moon'un düz Lua karşılığı).
-- Özgün: Copyright (c) 2013, Thomas Goyne (ISC/BSD).
-- Aegisub boost::regex (Perl sözdizimi) kullanır; burada Python `regex`
-- modülü (yoksa `re`) kullanılır. Konumlar her zaman UTF-8 BAYT konumudur,
-- Aegisub'daki gibi.

local py = __osisub_py

local FLAG_MT = {__tostring = function(f) return "re flag " .. f.name end}
local function is_flag(v) return type(v) == "table" and getmetatable(v) == FLAG_MT end

local flag_values = {
  ICASE = 1, NOSUB = 2, COLLATE = 4, NEWLINE_ALT = 8,
  NO_MOD_M = 16, NO_MOD_S = 32, MOD_X = 64, NO_EMPTY_SUBEXPRESSIONS = 128,
}

local function select_first(n, a, ...)
  if n == 0 then return end
  return a, select_first(n - 1, ...)
end

local function process_flags(...)
  local flags = 0
  for i = 1, select('#', ...) do
    local v = select(i, ...)
    if not is_flag(v) then error('Flags must follow all non-flag arguments', 3) end
    if math.floor(flags / v.value) % 2 == 0 then flags = flags + v.value end
  end
  return flags
end

local function unpack_args(...)
  local flags_start
  for i = 1, select('#', ...) do
    if is_flag((select(i, ...))) then flags_start = i break end
  end
  if not flags_start then return 0, ... end
  return process_flags(select(flags_start, ...)), select_first(flags_start - 1, ...)
end

local RegEx = {}
RegEx.__index = RegEx

local function search(h, str, start)
  if start > #str then return end
  return py.re_search(h, str, start)
end

local function do_replace_fun(self, func, str, max)
  local acc = {}
  local pos = 1
  for _ = 1, max do
    local matches = self:match(str, pos)
    if not matches then break end
    local first_i = (#matches == 1) and 1 or 2
    local last = pos
    local first
    for i = first_i, #matches do
      local m = matches[i]
      if last < m.last then acc[#acc + 1] = str:sub(last, m.first - 1) end
      local repl = func(m.str, m.first, m.last)
      acc[#acc + 1] = (type(repl) == "string") and repl or m.str
      first, last = m.first, m.last + 1
    end
    if first == last then
      acc[#acc + 1] = str:sub(last, last)
      last = last + 1
    end
    pos = last
    if matches[1].first > #str then break end
  end
  return table.concat(acc, '') .. str:sub(pos)
end

local function check_self(self)
  if getmetatable(self) ~= RegEx then
    error('re method called with invalid self. You probably used . when : is needed.', 3)
  end
end

function RegEx:gsplit(str, skip_empty, max_split)
  check_self(self)
  if not max_split or max_split <= 0 then max_split = #str end
  local start, prev = 0, 1
  local h = self._regex
  local function do_split()
    if not str or #str == 0 then return end
    local first, last
    if max_split > 0 then first, last = search(h, str, start) end
    if not first or first > #str then
      local ret = str:sub(prev, #str)
      str = nil
      if skip_empty and #ret == 0 then return nil end
      return ret
    end
    local ret = str:sub(prev, first - 1)
    prev = last + 1
    if start >= last then start = start + 1 else start = last end
    if skip_empty and #ret == 0 then
      return do_split()
    end
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
  check_self(self)
  local start = 0
  local h = self._regex
  return function()
    local first, last = search(h, str, start)
    if not first then return end
    if last > start then start = last else start = start + 1 end
    return str:sub(first, last), first, last
  end
end

function RegEx:find(str)
  local ret = {}
  for s, f, l in self:gfind(str) do ret[#ret + 1] = {str = s, first = f, last = l} end
  if next(ret) then return ret end
  return nil
end

function RegEx:sub(str, repl, max_count)
  check_self(self)
  if not max_count or max_count == 0 then max_count = #str + 1 end
  if type(repl) == "function" then
    return do_replace_fun(self, repl, str, max_count)
  elseif type(repl) == "string" then
    return py.re_replace(self._regex, repl, str, max_count)
  end
  error("re.sub: değiştirme metin ya da işlev olmalı", 2)
end

function RegEx:gmatch(str, start)
  check_self(self)
  start = start and (start - 1) or 0
  local m = nil
  if start <= #str then m = py.re_match(self._regex, str, start) end
  local i = 0
  return function()
    if not m then return end
    i = i + 1
    local pair = m[i]
    if not pair then return end
    return {str = str:sub(pair[1], pair[2]), first = pair[1], last = pair[2]}
  end
end

function RegEx:match(str, start)
  local ret = {}
  for v in self:gmatch(str, start) do ret[#ret + 1] = v end
  if next(ret) == nil then return nil end
  return ret
end

local function real_compile(pattern, level, flags)
  if pattern == '' then error('Regular expression must not be empty', level + 1) end
  local h, err = py.re_compile(pattern, flags)
  if not h then error(err, level + 1) end
  return setmetatable({_regex = h}, RegEx)
end

local function invoke(str, pattern, fn, flags, ...)
  local compiled = real_compile(pattern, 3, flags)
  return compiled[fn](compiled, str, ...)
end

local function gen_wrapper(name)
  return function(str, pattern, ...)
    if type(str) ~= "string" or type(pattern) ~= "string" then
      error("re." .. name .. ": metin ve desen gerekli", 2)
    end
    return invoke(str, pattern, name, unpack_args(...))
  end
end

local re = {
  compile = function(pattern, ...) return real_compile(pattern, 2, process_flags(...)) end,
  split = gen_wrapper('split'),
  gsplit = gen_wrapper('gsplit'),
  find = gen_wrapper('find'),
  gfind = gen_wrapper('gfind'),
  match = gen_wrapper('match'),
  gmatch = gen_wrapper('gmatch'),
  sub = gen_wrapper('sub'),
}
for name, value in pairs(flag_values) do
  re[name] = setmetatable({name = name, value = value}, FLAG_MT)
end
return re
