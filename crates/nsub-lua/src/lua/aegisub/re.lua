-- aegisub.re: port of Aegisub's re.moon on top of native regex primitives (fancy-regex).
-- Public API mirrors Aegisub: re.find/gfind/match/gmatch/sub/split/gsplit(str, pattern, flags...) and re.compile.
local impl = __nsub_re
local error, next, select, type = error, next, select, type

local flag_names = { "ICASE", "NOSUB", "COLLATE", "NEWLINE_ALT", "NO_MOD_M", "NO_MOD_S", "MOD_S", "MOD_X", "NO_EMPTY_SUBEXPRESSIONS" }
local flag_mt = {}
local function is_flag(v) return type(v) == "table" and getmetatable(v) == flag_mt end

local function process_flags(...)
  local flags = 0
  for i = 1, select("#", ...) do
    local v = select(i, ...)
    if not is_flag(v) then error("Flags must follow all non-flag arguments", 3) end
    flags = bit.bor(flags, v.value)
  end
  return flags
end

local function select_first(n, a, ...)
  if n == 0 then return end
  return a, select_first(n - 1, ...)
end

local function unpack_args(...)
  local flags_start
  for i = 1, select("#", ...) do
    if is_flag((select(i, ...))) then flags_start = i; break end
  end
  if not flags_start then return 0, ... end
  return process_flags(select(flags_start, ...)), select_first(flags_start - 1, ...)
end

-- search: 1-based first/last (inclusive) of next match at or after 0-based byte offset `start`
local function search(re, str, start)
  if start > #str then return end
  return impl.search(re, str, start)
end

local RegEx = {}
RegEx.__index = RegEx

local function new_regex(re) return setmetatable({ _regex = re }, RegEx) end

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
      if skip_empty and #ret == 0 then return nil else return ret end
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
  local r = {}
  for v in self:gsplit(str, skip_empty, max_split) do r[#r + 1] = v end
  return r
end

function RegEx:gfind(str)
  local start = 0
  return function()
    local first, last = search(self._regex, str, start)
    if not first then return end
    if last > start then start = last else start = start + 1 end
    return str:sub(first, last), first, last
  end
end

function RegEx:find(str)
  local ret = {}
  for s, f, l in self:gfind(str) do ret[#ret + 1] = { str = s, first = f, last = l } end
  if next(ret) then return ret end
end

function RegEx:gmatch(str, start)
  start = start and (start - 1) or 0
  if start > #str then return function() end end
  local m = impl.match(self._regex, str, start)
  local i = 0
  return function()
    if not m then return end
    local first, last = impl.group(m, i)
    if not first then return end
    i = i + 1
    return { str = str:sub(first, last), first = first, last = last }
  end
end

function RegEx:match(str, start)
  local ret = {}
  for v in self:gmatch(str, start) do ret[#ret + 1] = v end
  if next(ret) == nil then return nil end
  return ret
end

local function replace_match(match, func, str, last, acc)
  if last < match.last then acc[#acc + 1] = str:sub(last, match.first - 1) end
  local repl = func(match.str, match.first, match.last)
  acc[#acc + 1] = type(repl) == "string" and repl or match.str
  return match.first, match.last + 1
end

local function do_single_replace_fun(re, func, str, acc, pos)
  local matches = re:match(str, pos)
  if not matches then return pos end
  local start = (#matches == 1) and 1 or 2
  local last = pos
  local first
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

function RegEx:sub(str, repl, max_count)
  if not max_count or max_count == 0 then max_count = #str + 1 end
  if type(repl) == "function" then
    return do_replace_fun(self, repl, str, max_count)
  elseif type(repl) == "string" then
    return impl.replace(self._regex, repl, str, max_count)
  end
end

local function real_compile(pattern, level, flags)
  if pattern == "" then error("Regular expression must not be empty", level + 1) end
  local re, err = impl.compile(pattern, flags)
  if not re then error(err, level + 1) end
  return new_regex(re)
end

local function invoke(str, pattern, fn, flags, ...)
  local compiled = real_compile(pattern, 3, flags)
  return compiled[fn](compiled, str, ...)
end

local function gen_wrapper(name)
  return function(str, pattern, ...) return invoke(str, pattern, name, unpack_args(...)) end
end

local re = {
  compile = function(pattern, ...) return real_compile(pattern, 2, process_flags(...)) end,
  split = gen_wrapper("split"),
  gsplit = gen_wrapper("gsplit"),
  find = gen_wrapper("find"),
  gfind = gen_wrapper("gfind"),
  match = gen_wrapper("match"),
  gmatch = gen_wrapper("gmatch"),
  sub = gen_wrapper("sub"),
}
for i, name in ipairs(flag_names) do
  re[name] = setmetatable({ name = name, value = 2 ^ (i - 1) }, flag_mt)
end
for _, n in ipairs(flag_names) do re[n].value = math.floor(re[n].value) end

return re
