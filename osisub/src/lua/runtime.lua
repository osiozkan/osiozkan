-- osisub Aegisub Automation 4 compatibility runtime (Lua 5.3 host, LuaJIT/5.1 source compatibility).
-- Native hooks are provided in the global table __host by the JS side.

local host = __host
__host = nil

---------------------------------------------------------------------------
-- Lua 5.1 / LuaJIT compatibility shims
---------------------------------------------------------------------------
unpack = unpack or table.unpack
loadstring = loadstring or load
table.getn = table.getn or function(t) return #t end
table.maxn = table.maxn or function(t) local n = 0 for k in pairs(t) do if type(k) == "number" and k > n then n = k end end return n end
math.mod = math.mod or math.fmod
math.pow = math.pow or function(a, b) return a ^ b end
math.ldexp = math.ldexp or function(m, e) return m * 2.0 ^ e end
math.log10 = math.log10 or function(x) return math.log(x, 10) end
math.cosh = math.cosh or function(x) return (math.exp(x) + math.exp(-x)) / 2 end
math.sinh = math.sinh or function(x) return (math.exp(x) - math.exp(-x)) / 2 end
math.tanh = math.tanh or function(x) local a, b = math.exp(x), math.exp(-x) return (a - b) / (a + b) end
string.gfind = string.gfind or string.gmatch
if not newproxy then newproxy = function() return setmetatable({}, {}) end end

-- LuaJIT's %d accepts non-integral numbers (truncates); Lua 5.3 errors.
do
  local fmt = string.format
  string.format = function(f, ...)
    local args = table.pack(...)
    local i = 0
    for spec in f:gmatch("%%[%-%+ #0]*[%d%*]*%.?%d*([%a%%])") do
      if spec ~= "%" then
        i = i + 1
        local a = args[i]
        if (spec == "d" or spec == "i" or spec == "x" or spec == "X" or spec == "o" or spec == "c" or spec == "u") and type(a) == "number" then
          if math.type(a) == "float" then
            if a ~= a or a == math.huge or a == -math.huge then a = 0 else a = a >= 0 and math.floor(a) or -math.floor(-a) end
            args[i] = a
          end
        end
        if spec == "u" then end
      end
    end
    f = f:gsub("%%([%-%+ #0]*[%d%*]*%.?%d*)u", "%%%1d")
    return fmt(f, table.unpack(args, 1, args.n))
  end
end

-- setfenv/getfenv (5.1) on top of 5.2+ upvalue tricks
if not setfenv then
  local function findenv(f)
    local i = 1
    while true do
      local name, val = debug.getupvalue(f, i)
      if name == "_ENV" then return i, val elseif not name then break end
      i = i + 1
    end
  end
  function setfenv(f, t)
    if type(f) == "number" then f = debug.getinfo(f + 1, "f").func end
    local i = findenv(f)
    if i then debug.upvaluejoin(f, i, function() return t end, 1) end
    return f
  end
  function getfenv(f)
    if type(f) == "number" then f = debug.getinfo(f + 1, "f").func end
    if f == nil then return _G end
    local i, val = findenv(f)
    return val or _G
  end
end

-- bit library (LuaJIT)
bit = bit or {
  tobit = function(x) x = math.tointeger(x) or math.floor(x); x = x & 0xffffffff; if x >= 0x80000000 then x = x - 0x100000000 end return x end,
  band = function(a, b, ...) local r = (math.tointeger(a) or math.floor(a)) & (math.tointeger(b) or math.floor(b)); if ... then return bit.band(r, ...) end return bit.tobit(r) end,
  bor = function(a, b, ...) local r = (math.tointeger(a) or math.floor(a)) | (math.tointeger(b) or math.floor(b)); if ... then return bit.bor(r, ...) end return bit.tobit(r) end,
  bxor = function(a, b, ...) local r = (math.tointeger(a) or math.floor(a)) ~ (math.tointeger(b) or math.floor(b)); if ... then return bit.bxor(r, ...) end return bit.tobit(r) end,
  bnot = function(a) return bit.tobit(~(math.tointeger(a) or math.floor(a))) end,
  lshift = function(a, n) return bit.tobit((math.tointeger(a) or math.floor(a)) << n) end,
  rshift = function(a, n) return bit.tobit(((math.tointeger(a) or math.floor(a)) & 0xffffffff) >> n) end,
  arshift = function(a, n) a = bit.tobit(a) return bit.tobit(a >> n | (a < 0 and ~(0xffffffff >> n) or 0)) end,
  tohex = function(a, n) return string.format("%0" .. (n or 8) .. "x", a & 0xffffffff):sub(-(n or 8)) end,
}

-- string helpers Aegisub scripts commonly assume
string.trim = string.trim or function(s) return (s:gsub("^%s+", ""):gsub("%s+$", "")) end
string.headtail = string.headtail or function(s)
  local a, b, head, tail = s:find("^(%S+)%s+(.*)$")
  if a then return head, tail end
  return s, ""
end
string.words = string.words or function(s)
  return coroutine.wrap(function() for w in s:gmatch("%S+") do coroutine.yield(w) end end)
end
math.round = math.round or function(x, p)
  if p then local m = 10 ^ p return math.floor(x * m + 0.5) / m end
  return math.floor(x + 0.5)
end

---------------------------------------------------------------------------
-- io / os (file access through the host; fengari's own io is not usable)
---------------------------------------------------------------------------
do
  local FH = {}
  FH.__index = FH
  FH.__name = "FILE*"
  local function readfmt(self, fmt)
    local d = self.data
    if fmt == "*a" or fmt == "a" then local r = d:sub(self.pos); self.pos = #d + 1; return r end
    if fmt == "*l" or fmt == "l" or fmt == "*L" or fmt == "L" then
      if self.pos > #d then return nil end
      local e = d:find("\n", self.pos, true)
      local line
      if e then line = d:sub(self.pos, (fmt:find("L") and e or e - 1)); self.pos = e + 1
      else line = d:sub(self.pos); self.pos = #d + 1 end
      if not fmt:find("L") then line = line:gsub("\r$", "") end
      return line
    end
    if fmt == "*n" or fmt == "n" then
      local a, b = d:find("^%s*[-+]?%d*%.?%d+[eE]?[-+]?%d*", self.pos)
      if not a then return nil end
      self.pos = b + 1
      return tonumber(d:sub(a, b))
    end
    if type(fmt) == "number" then
      if self.pos > #d then return nil end
      local r = d:sub(self.pos, self.pos + fmt - 1); self.pos = self.pos + #r; return r
    end
    error("bad argument to 'read' (invalid format)", 3)
  end
  function FH:read(...)
    if self.closed then error("attempt to use a closed file", 2) end
    local n = select("#", ...)
    if n == 0 then return readfmt(self, "*l") end
    local out = {}
    for i = 1, n do out[i] = readfmt(self, (select(i, ...))); if out[i] == nil then break end end
    return table.unpack(out, 1, n)
  end
  function FH:lines(...)
    local fmts = { ... }
    return function() return self:read(table.unpack(fmts)) end
  end
  function FH:write(...)
    if self.closed then error("attempt to use a closed file", 2) end
    if not self.writable then return nil, "file not opened for writing", 9 end
    for i = 1, select("#", ...) do
      local v = select(i, ...)
      self.buf[#self.buf + 1] = tostring(v)
    end
    self.dirty = true
    return self
  end
  function FH:flush()
    if self.writable and self.dirty then
      host.fs_write(self.path, table.concat(self.buf), self.appended)
      self.buf = {}; self.appended = true; self.dirty = false
    end
    return self
  end
  function FH:close() if not self.closed then self:flush(); self.closed = true end return true end
  function FH:seek(whence, off)
    whence = whence or "cur"; off = off or 0
    if whence == "set" then self.pos = off + 1 elseif whence == "cur" then self.pos = self.pos + off else self.pos = #self.data + off + 1 end
    return self.pos - 1
  end
  function FH:setvbuf() return true end
  FH.__gc = function(self) pcall(self.close, self) end
  FH.__tostring = function(self) return self.closed and "file (closed)" or "file (0x1)" end

  local function open(path, mode)
    mode = mode or "r"
    local m = mode:gsub("b", "")
    local self = setmetatable({ path = path, pos = 1, buf = {}, data = "", writable = false }, FH)
    if m == "r" or m == "r+" then
      local d = host.fs_read(path)
      if d == nil then return nil, path .. ": No such file or directory", 2 end
      self.data = d; self.writable = (m == "r+")
    elseif m == "w" or m == "w+" then
      self.writable = true; self.appended = false; self.dirty = true
      if not host.fs_write(path, "", false) then return nil, path .. ": Permission denied", 13 end
      self.appended = true
    elseif m == "a" or m == "a+" then
      self.writable = true; self.appended = true
    else
      error("bad argument #2 to 'open' (invalid mode)", 2)
    end
    return self
  end
  io = io or {}
  io.open = open
  function io.lines(path, ...)
    if not path then return function() return nil end end
    local f, err = open(path, "r")
    if not f then error(err, 2) end
    local fmts = { ... }
    return function()
      local r = f:read(table.unpack(fmts))
      if r == nil then f:close() end
      return r
    end
  end
  function io.close(f) if f then return f:close() end return true end
  function io.type(f) if getmetatable(f) == FH then return f.closed and "closed file" or "file" end return nil end
  function io.write(...) for i = 1, select("#", ...) do host.stdout(tostring((select(i, ...)))) end return io.stdout end
  function io.read() return nil end
  io.stdout = { write = function(self, ...) io.write(...) return self end, flush = function() end, close = function() end, setvbuf = function() end }
  io.stderr = { write = function(self, ...) for i = 1, select("#", ...) do host.stderr(tostring((select(i, ...)))) end return self end, flush = function() end, close = function() end, setvbuf = function() end }
  io.stdin = { read = function() return nil end, lines = function() return function() return nil end end, close = function() end }
  io.output = function() return io.stdout end
  io.input = function() return io.stdin end

  os = os or {}
  os.remove = function(p) if host.fs_remove(p) then return true end return nil, p .. ": No such file or directory", 2 end
  os.rename = function(a, b) if host.fs_rename(a, b) then return true end return nil, a .. ": No such file or directory", 2 end
  os.tmpname = function() return host.fs_tmpname() end
  os.getenv = function(k) return host.getenv(k) end
  os.execute = function() return nil, "exit", 1 end
  os.exit = function() error("os.exit is not available inside macros", 2) end
  print = function(...)
    local t = {}
    for i = 1, select("#", ...) do t[i] = tostring((select(i, ...))) end
    host.stdout(table.concat(t, "\t") .. "\n")
  end
end

---------------------------------------------------------------------------
-- aegisub namespace
---------------------------------------------------------------------------
aegisub = {}
aegisub.lua_automation_version = 4
aegisub.__macros, aegisub.__filters, aegisub.__exporters = {}, {}, {}

local function tostr(...)
  local n = select("#", ...)
  local t = {}
  for i = 1, n do t[i] = tostring((select(i, ...))) end
  return table.concat(t, "\t")
end

function aegisub.log(a, b, ...)
  local level, msg = 2, a
  if type(a) == "number" and type(b) == "string" then level, msg = a, b end
  if type(a) == "number" and type(b) == "string" then
    if select("#", ...) > 0 then msg = msg:format(...) end
  elseif type(a) == "string" and b ~= nil then
    msg = a:format(b, ...)
  end
  host.log(level, tostring(msg))
end
aegisub.debug = { out = function(level, fmt, ...)
  if type(level) == "string" then return aegisub.log(5, level, fmt, ...) end
  return aegisub.log(level, fmt, ...)
end }

function aegisub.register_macro(name, description, run, validate, is_active)
  assert(type(name) == "string", "macro name must be a string")
  assert(type(run) == "function", "macro '" .. name .. "' needs a function")
  table.insert(aegisub.__macros, { name = name, description = description, run = run, validate = validate, is_active = is_active })
end
function aegisub.register_filter(name, description, priority, run, config)
  table.insert(aegisub.__filters, { name = name, description = description, priority = priority, run = run, config = config })
end
function aegisub.register_exporter(...) return aegisub.register_filter(...) end

aegisub.progress = {
  set = function(p) host.progress(p) end,
  task = function(t) host.task(tostring(t)) end,
  title = function(t) host.title(tostring(t)) end,
}
function aegisub.cancel() error("__aegisub_cancel__", 0) end
function aegisub.cancelled() return false end
function aegisub.gettext(s) return s end
function aegisub.text_extents(style, text)
  local s = style
  return host.text_extents(tostring(s.fontname or "Arial"), tonumber(s.fontsize) or 48, s.bold and true or false, s.italic and true or false,
    tonumber(s.scale_x) or 100, tonumber(s.scale_y) or 100, tonumber(s.spacing) or 0, tostring(text))
end
function aegisub.frame_from_ms(ms) return host.frame_from_ms(ms) end
function aegisub.ms_from_frame(f) return host.ms_from_frame(f) end
function aegisub.video_size()
  local w, h, ar, art = host.video_size()
  if not w then return nil end
  return w, h, ar, art
end
function aegisub.keyframes() return host.keyframes() end
function aegisub.decode_path(p) return host.decode_path(p) end
function aegisub.get_audio_selection() return nil end
function aegisub.audio_closest_markers() return nil end
function aegisub.project_properties() return host.project_properties() end
function aegisub.file_name() return host.file_name() end
aegisub.clipboard = { get = function() return host.clipboard_get() end, set = function(s) host.clipboard_set(s) return true end }
aegisub.cancelled = function() return false end
aegisub.dialog = {}
aegisub.ui = {}

local function default_value(c)
  if c.class == "label" then return nil end
  if c.class == "checkbox" then return c.value and true or false end
  if c.class == "dropdown" then return c.value or (c.items and c.items[1]) or "" end
  if c.class == "intedit" or c.class == "floatedit" or c.class == "slider" then return c.value or c.min or 0 end
  if c.class == "color" or c.class == "coloralpha" or c.class == "alpha" then return c.value or "" end
  if c.class == "textbox" or c.class == "edit" or c.class == "selecttable" then return c.value or c.text or "" end
  return c.value or c.text
end

function aegisub.dialog.display(dialog, buttons, button_ids)
  buttons = buttons or { "OK", "Cancel" }
  button_ids = button_ids or {}
  local ov = host.dialog_response(dialog, buttons, button_ids) -- may abort the macro to ask the UI
  local result = {}
  for _, c in ipairs(dialog or {}) do
    if c.name then
      local v = default_value(c)
      if ov.values[c.name] ~= nil then
        v = ov.values[c.name]
        if c.class == "checkbox" then v = (v == true or v == "true" or v == "1" or v == 1 or v == "yes") end
        if c.class == "intedit" or c.class == "floatedit" or c.class == "slider" then v = tonumber(v) or v end
      end
      result[c.name] = v
    end
  end
  local pick = ov.button
  local ok = button_ids.ok or buttons[1]
  if pick == "cancel" or pick == false then return false, result end
  if type(pick) == "string" and pick ~= "ok" then
    for _, b in ipairs(buttons) do if b == pick then return b, result end end
  end
  if #buttons == 0 then return true, result end
  if buttons[1] == "OK" or button_ids.ok then return (button_ids.ok and button_ids.ok or true), result end
  return buttons[1], result
end
function aegisub.dialog.open(title, default_name, dir, wildcards, multiple, must_exist)
  local p = host.dialog_overrides().open
  return p
end
function aegisub.dialog.save(title, default_name, dir, wildcards, overwrite_prompt)
  return host.dialog_overrides().save
end
aegisub.dialog.open = aegisub.dialog.open

---------------------------------------------------------------------------
-- Subtitle object helpers
---------------------------------------------------------------------------
-- Aegisub calls these with a dot (subs.append(line)), not a colon, so bind to the table.
local subs_methods = {}
function subs_methods.append(self, ...)
  for i = 1, select("#", ...) do self[#self + 1] = (select(i, ...)) end
end
function subs_methods.insert(self, idx, ...)
  local n = select("#", ...)
  for i = 1, n do table.insert(self, idx + i - 1, (select(i, ...))) end
end
function subs_methods.delete(self, ...)
  local idx = { ... }
  if type(idx[1]) == "table" then idx = idx[1] end
  table.sort(idx, function(a, b) return a > b end)
  for _, i in ipairs(idx) do table.remove(self, i) end
end
function subs_methods.deleterange(self, a, b) for i = b, a, -1 do table.remove(self, i) end end
function subs_methods.get_extradata() return {} end
function subs_methods.set_extradata() end
__subs_mt = {
  __index = function(t, k)
    local f = subs_methods[k]
    if f then return function(...) return f(t, ...) end end
    if k == "n" then return #t end
  end,
}

---------------------------------------------------------------------------
-- util module
---------------------------------------------------------------------------
local util = {}

function util.copy(t)
  if type(t) ~= "table" then return t end
  local r = {}
  for k, v in pairs(t) do r[k] = v end
  return r
end
function util.deep_copy(t)
  if type(t) ~= "table" then return t end
  local seen = {}
  local function cp(o)
    if type(o) ~= "table" then return o end
    if seen[o] then return seen[o] end
    local r = {}
    seen[o] = r
    for k, v in pairs(o) do r[cp(k)] = cp(v) end
    return setmetatable(r, getmetatable(o))
  end
  return cp(t)
end
function util.clamp(v, lo, hi) return math.min(math.max(v, lo), hi) end
function util.float_str(n)
  if n == math.floor(n) then return tostring(math.floor(n)) end
  return (string.format("%.3f", n):gsub("0+$", ""):gsub("%.$", ""))
end
function util.ass_alpha(a) return string.format("&H%02X&", util.clamp(math.floor(a + 0.5), 0, 255)) end
function util.ass_color(r, g, b) return string.format("&H%02X%02X%02X&", util.clamp(math.floor(b + .5), 0, 255), util.clamp(math.floor(g + .5), 0, 255), util.clamp(math.floor(r + .5), 0, 255)) end
function util.ass_style_color(r, g, b) return string.format("&H00%02X%02X%02X", util.clamp(math.floor(b + .5), 0, 255), util.clamp(math.floor(g + .5), 0, 255), util.clamp(math.floor(r + .5), 0, 255)) end
function util.extract_color(s)
  local a, b, g, r = 0, 0, 0, 0
  local h = tostring(s):match("&?[Hh]?(%x+)&?")
  if not h then return 0, 0, 0, 0 end
  h = ("0"):rep(8 - #h) .. h
  a, b, g, r = tonumber(h:sub(1, 2), 16), tonumber(h:sub(3, 4), 16), tonumber(h:sub(5, 6), 16), tonumber(h:sub(7, 8), 16)
  return r, g, b, a
end
function util.alpha_from_style(c) local _, _, _, a = util.extract_color(c) return util.ass_alpha(a) end
function util.color_from_style(c) local r, g, b = util.extract_color(c) return util.ass_color(r, g, b) end
function util.interpolate(t, a, b) return a + (b - a) * t end
function util.interpolate_alpha(t, a, b)
  local _, _, _, a1 = util.extract_color(a)
  local _, _, _, a2 = util.extract_color(b)
  return util.ass_alpha(util.interpolate(t, a1, a2))
end
function util.interpolate_color(t, c1, c2)
  local r1, g1, b1 = util.extract_color(c1)
  local r2, g2, b2 = util.extract_color(c2)
  return util.ass_color(util.interpolate(t, r1, r2), util.interpolate(t, g1, g2), util.interpolate(t, b1, b2))
end
function util.rgb_to_hsv(r, g, b)
  r, g, b = r / 255, g / 255, b / 255
  local mx, mn = math.max(r, g, b), math.min(r, g, b)
  local d, h = mx - mn, 0
  if d > 0 then
    if mx == r then h = ((g - b) / d) % 6 elseif mx == g then h = (b - r) / d + 2 else h = (r - g) / d + 4 end
    h = h * 60
  end
  return h, mx == 0 and 0 or d / mx, mx
end
function util.hsv_to_rgb(h, s, v)
  h = h % 360
  local c = v * s
  local x = c * (1 - math.abs((h / 60) % 2 - 1))
  local m = v - c
  local r, g, b = 0, 0, 0
  if h < 60 then r, g, b = c, x, 0 elseif h < 120 then r, g, b = x, c, 0 elseif h < 180 then r, g, b = 0, c, x
  elseif h < 240 then r, g, b = 0, x, c elseif h < 300 then r, g, b = x, 0, c else r, g, b = c, 0, x end
  return math.floor((r + m) * 255 + .5), math.floor((g + m) * 255 + .5), math.floor((b + m) * 255 + .5)
end
util.HSV_to_RGB = function(h, s, v) return util.hsv_to_rgb(h, s / 255, v / 255) end
util.RGB_to_HSV = function(r, g, b) local h, s, v = util.rgb_to_hsv(r, g, b) return h, s * 255, v * 255 end
function util.HSL_to_RGB(h, s, l)
  s, l = s / 255, l / 255
  local c = (1 - math.abs(2 * l - 1)) * s
  local x = c * (1 - math.abs((h / 60) % 2 - 1))
  local m = l - c / 2
  local r, g, b = 0, 0, 0
  h = h % 360
  if h < 60 then r, g, b = c, x, 0 elseif h < 120 then r, g, b = x, c, 0 elseif h < 180 then r, g, b = 0, c, x
  elseif h < 240 then r, g, b = 0, x, c elseif h < 300 then r, g, b = x, 0, c else r, g, b = c, 0, x end
  return math.floor((r + m) * 255 + .5), math.floor((g + m) * 255 + .5), math.floor((b + m) * 255 + .5)
end
function util.fix_fnt(f) return f end
function util.uuid() return host.uuid() end
function util.trim(s) return string.trim(s) end
util.headtail = string.headtail
util.words = string.words
function util.split(s, sep)
  local t = {}
  for p in (s .. sep):gmatch("(.-)" .. sep:gsub("%p", "%%%0")) do t[#t + 1] = p end
  return t
end

function table.copy(t) return util.copy(t) end
function table.copy_deep(t) return util.deep_copy(t) end
function table.append(a, b) for _, v in ipairs(b) do a[#a + 1] = v end return a end
function table.isempty(t) return next(t) == nil end
function table.indexof(t, v) for i, x in ipairs(t) do if x == v then return i end end end
function table.contains(t, v) return table.indexof(t, v) ~= nil end

---------------------------------------------------------------------------
-- unicode / re modules
---------------------------------------------------------------------------
local unicode = {}
function unicode.codepoint(s) return utf8.codepoint(s, 1) end
function unicode.len(s) return utf8.len(s) or #s end
function unicode.chars(s)
  return coroutine.wrap(function()
    for _, c in utf8.codes(s) do coroutine.yield(utf8.char(c)) end
  end)
end
function unicode.charwidth(c) return 1 end
function unicode.sub(s, i, j)
  local n = utf8.len(s)
  i = i or 1; j = j or n
  if i < 0 then i = n + i + 1 end
  if j < 0 then j = n + j + 1 end
  if i < 1 then i = 1 end
  if j > n then j = n end
  if i > j then return "" end
  return s:sub(utf8.offset(s, i), utf8.offset(s, j + 1) - 1)
end
function unicode.to_upper(s) return host.re_upper(s) end
function unicode.to_lower(s) return host.re_lower(s) end
function unicode.to_fold_case(s) return host.re_lower(s) end

local re = {}
local function wrap_match(m)
  if not m then return nil end
  local out = {}
  for i, g in ipairs(m) do out[i] = { str = g.str, first = g.first, last = g.last } end
  return out
end
function re.find(s, pat, flags) return wrap_match(host.re_find(s, pat, flags or "")) end
function re.match(s, pat, flags) return wrap_match(host.re_match(s, pat, flags or "")) end
function re.sub(s, pat, rep, count, flags) return host.re_sub(s, pat, rep, count or 0, flags or "") end
function re.gsub(s, pat, rep, count, flags) return host.re_sub(s, pat, rep, count or 0, flags or "") end
function re.gfind(s, pat, flags)
  local all = re.find(s, pat, flags)
  local i = 0
  return function() i = i + 1; return all and all[i] end
end
re.gmatch = re.gfind
function re.compile(pat, flags)
  local o = {}
  function o:find(s) return re.find(s, pat, flags) end
  function o:match(s) return re.match(s, pat, flags) end
  function o:sub(s, rep, n) return re.sub(s, pat, rep, n, flags) end
  o.gsub = o.sub
  return o
end

---------------------------------------------------------------------------
-- karaskel (the commonly used subset)
---------------------------------------------------------------------------
local karaskel = {}

function karaskel.collect_head(subs, generate_furigana)
  local meta = { res_x = 0, res_y = 0 }
  local styles = { n = 0 }
  local i = 1
  while i <= #subs do
    local l = subs[i]
    if l.class == "info" then
      local k = l.key:lower()
      if k == "playresx" then meta.res_x = math.floor(tonumber(l.value) or 0)
      elseif k == "playresy" then meta.res_y = math.floor(tonumber(l.value) or 0)
      elseif k == "wrapstyle" then meta.wrap_style = tonumber(l.value)
      elseif k == "scaledborderandshadow" then meta.scaled_border_and_shadow = l.value:lower() == "yes" end
      meta[l.key] = l.value
    elseif l.class == "style" then
      styles.n = styles.n + 1
      styles[styles.n] = l
      styles[l.name] = l
      l.id = styles.n
    elseif l.class == "dialogue" then
      break
    end
    i = i + 1
  end
  if meta.res_x == 0 and meta.res_y == 0 then meta.res_x, meta.res_y = 384, 288
  elseif meta.res_x == 0 then meta.res_x = meta.res_y / 3 * 4
  elseif meta.res_y == 0 then meta.res_y = meta.res_x / 4 * 3 end
  for _, s in ipairs(styles) do karaskel.parse_style_extra(s) end
  return meta, styles
end
function karaskel.parse_style_extra(style) end
function karaskel.collect_head_only(subs) return karaskel.collect_head(subs) end

function karaskel.preproc_line_text(meta, styles, line)
  line.text_stripped = line.text:gsub("%b{}", "")
  line.duration = line.end_time - line.start_time
  line.kara = { n = 0 }
  line.styleref = styles[line.style] or styles["Default"] or styles[1]
end

function karaskel.preproc_line_size(meta, styles, line)
  local style = line.styleref
  local width, height, descent, ext_lead = aegisub.text_extents(style, line.text_stripped)
  line.width, line.height, line.descent, line.extlead = width, height, descent, ext_lead
  line.margin_v = line.margin_t
  line.margin_l = line.margin_l == 0 and style.margin_l or line.margin_l
  line.margin_r = line.margin_r == 0 and style.margin_r or line.margin_r
  line.margin_t = line.margin_t == 0 and style.margin_t or line.margin_t
  line.margin_b = line.margin_b == 0 and style.margin_b or line.margin_b
  line.margin_v = line.margin_t
end

function karaskel.preproc_line_pos(meta, styles, line)
  local style = line.styleref
  local align = style.align
  local an = line.text:match("{[^}]-\\an(%d)")
  if an then align = tonumber(an) end
  local h = (align - 1) % 3
  if h == 0 then line.halign = "left" elseif h == 1 then line.halign = "center" else line.halign = "right" end
  local v = math.floor((align - 1) / 3)
  if v == 0 then line.valign = "bottom" elseif v == 1 then line.valign = "middle" else line.valign = "top" end
  line.left = line.margin_l
  line.right = meta.res_x - line.margin_r
  line.top = line.margin_t
  line.bottom = meta.res_y - line.margin_b
  line.vtop = line.top
  line.vbottom = line.bottom
  line.vmiddle = meta.res_y / 2
  line.vcenter = line.vmiddle
  line.hcenter = meta.res_x / 2
  line.hmiddle = line.hcenter
  if line.halign == "left" then line.x = line.left; elseif line.halign == "center" then line.x = line.hcenter; else line.x = line.right end
  if line.valign == "bottom" then line.y = line.bottom; elseif line.valign == "middle" then line.y = line.vmiddle; else line.y = line.top end
  if line.halign == "left" then line.left_edge = line.x
  elseif line.halign == "center" then line.left_edge = line.x - line.width / 2
  else line.left_edge = line.x - line.width end
end

function karaskel.preproc_line(subs, meta, styles, line)
  karaskel.preproc_line_text(meta, styles, line)
  karaskel.preproc_line_size(meta, styles, line)
  karaskel.preproc_line_pos(meta, styles, line)
end

function karaskel.parse_kara_syllables(line)
  -- minimal \k parser: fills line.kara with syllables
  line.kara = { n = 0 }
  local t = line.text
  local pos, time = 1, 0
  local cur
  local function push(text, dur, tag)
    local k = line.kara
    k.n = k.n + 1
    k[k.n] = { text = text, text_stripped = text:gsub("%b{}", ""), duration = dur, start_time = time, end_time = time + dur, tag = tag }
    time = time + dur
  end
  local pending_dur, pending_tag = 0, "k"
  local buf = ""
  for blk, txt in t:gmatch("({[^}]*})([^{]*)") do
    local tag, d = blk:match("\\([kK][fo]?)(%d+)")
    if tag then
      if buf ~= "" or pending_dur > 0 then push(buf, pending_dur * 10, pending_tag) end
      buf, pending_dur, pending_tag = txt, tonumber(d), tag
    else
      buf = buf .. blk .. txt
    end
  end
  if buf ~= "" or pending_dur > 0 then push(buf, pending_dur * 10, pending_tag) end
end

function karaskel.preproc_kara(subs, meta, styles, line) karaskel.parse_kara_syllables(line) end

---------------------------------------------------------------------------
-- include / require
---------------------------------------------------------------------------
local builtin = {
  ["utils.lua"] = function()
    for k, v in pairs(util) do if _G[k] == nil then _G[k] = v end end
    _G.util = util
    return util
  end,
  ["util.lua"] = function() _G.util = util return util end,
  ["karaskel.lua"] = function() _G.karaskel = karaskel return karaskel end,
  ["unicode.lua"] = function() _G.unicode = unicode return unicode end,
  ["re.lua"] = function() _G.re = re return re end,
  ["clipboard.lua"] = function() return aegisub.clipboard end,
}
package.preload["aegisub.util"] = function() return util end
package.preload["aegisub.unicode"] = function() return unicode end
package.preload["aegisub.re"] = function() return re end
package.preload["aegisub.clipboard"] = function() return aegisub.clipboard end
package.preload["aegisub.karaskel"] = function() return karaskel end
package.preload["util"] = function() return util end
package.preload["karaskel"] = function() return karaskel end
package.preload["unicode"] = function() return unicode end
package.preload["re"] = function() return re end
package.preload["lfs"] = function()
  return {
    attributes = function(p, k) return host.file_attr(p, k) end,
    currentdir = function() return host.cwd() end,
    dir = function() return function() return nil end end,
  }
end

local included = {}
function include(name)
  local b = builtin[name]
  if b then return b() end
  local src, path = host.read_include(name)
  if not src then error("Lua include not found: " .. name, 2) end
  if included[path] then return included[path] end
  local chunk, err = load(src, "@" .. path, "t", _G)
  if not chunk then error(err, 2) end
  local ret = chunk()
  included[path] = ret == nil and true or ret
  return ret
end

local real_require = require
function require(name)
  local ok, res = pcall(real_require, name)
  if ok then return res end
  local b = builtin[name .. ".lua"]
  if b then return b() end
  local src, path = host.read_include(name:gsub("%.", "/") .. ".lua")
  if src then
    local chunk, err = load(src, "@" .. path, "t", _G)
    if not chunk then error(err, 2) end
    local ret = chunk(name)
    package.loaded[name] = ret == nil and true or ret
    return package.loaded[name]
  end
  error(res, 2)
end

-- a few Aegisub-bundled globals available without include in recent versions
_G.util = util
_G.unicode = unicode
_G.re = re
_G.karaskel = karaskel
