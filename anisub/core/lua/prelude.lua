-- AniSub: Aegisub-compatible automation prelude.
-- Provides the `aegisub` table, subtitle-file proxy object, and registration functions.
local host = __host          -- table of native functions injected by Rust
local M = { macros = {}, filters = {} }
__anisub = M

aegisub = aegisub or {}
aegisub.lua_automation_version = 4
aegisub.progress = { set = function() end, task = function() end, title = function() end, is_cancelled = function() return false end }
aegisub.debug = {}
aegisub.dialog = {}

local function fmt_args(...)
  local n = select('#', ...)
  if n == 0 then return "" end
  local msg = ...
  if n == 1 then return tostring(msg) end
  local ok, r = pcall(string.format, ...)
  if ok then return r end
  return tostring(msg)
end

function aegisub.log(...)
  local first = ...
  if type(first) == "number" and select('#', ...) >= 2 then
    host.log(first, fmt_args(select(2, ...)) .. "")
  else
    host.log(2, fmt_args(...))
  end
end
function aegisub.debug.out(...)
  local first = ...
  if type(first) == "number" and select('#', ...) >= 2 then
    host.log(first, fmt_args(select(2, ...)))
  else
    host.log(2, fmt_args(...))
  end
end

function aegisub.cancel() error("__anisub_cancel__", 0) end
function aegisub.gettext(s) return s end
aegisub.gettext_noop = aegisub.gettext
function aegisub.text_extents(style, text) return host.text_extents(style, text) end
function aegisub.frame_from_ms(ms) return host.frame_from_ms(ms) end
function aegisub.ms_from_frame(f) return host.ms_from_frame(f) end
function aegisub.video_size() return host.video_size() end
function aegisub.keyframes() return host.keyframes() end
function aegisub.decode_path(p) return host.decode_path(p) end
function aegisub.project_properties() return host.project_properties() end
function aegisub.get_audio_selection() return nil end
function aegisub.get_cursor() return nil end
function aegisub.dialog.display(config, buttons, ids) return host.dialog_display(config, buttons, ids) end
function aegisub.dialog.open(...) return host.dialog_open(...) end
function aegisub.dialog.save(...) return host.dialog_save(...) end

function aegisub.register_macro(name, description, process, validate, is_active)
  table.insert(M.macros, { name = name, description = description, process = process, validate = validate, is_active = is_active })
end
function aegisub.register_filter(name, description, priority, process, config)
  table.insert(M.filters, { name = name, description = description, priority = priority, process = process, config = config })
end

-- Subtitle file proxy ---------------------------------------------------
local function copy_line(l)
  local c = {}
  for k, v in pairs(l) do c[k] = v end
  if l.extra then
    c.extra = {}
    for k, v in pairs(l.extra) do c.extra[k] = v end
  end
  return c
end

local function check_line(l)
  if type(l) ~= "table" then error("Attempt to insert a non-table value as a subtitle line", 3) end
  if not l.class then error("Subtitle line has no 'class' field", 3) end
end

function M.make_subs(data)
  local methods = {}
  local function chk(i)
    if type(i) ~= "number" or i < 1 or i > #data or i ~= math.floor(i) then
      error("Attempt to access a subtitle line out of range (index " .. tostring(i) .. ")", 3)
    end
  end
  function methods.insert(i, ...)
    local lines = { ... }
    if i < 1 or i > #data + 1 then error("Attempt to insert line out of range", 2) end
    for j = #lines, 1, -1 do check_line(lines[j]); table.insert(data, i, copy_line(lines[j])) end
  end
  function methods.append(...)
    for _, l in ipairs({ ... }) do check_line(l); data[#data + 1] = copy_line(l) end
  end
  function methods.delete(...)
    local idx = { ... }
    if #idx == 1 and type(idx[1]) == "table" then idx = idx[1] end
    table.sort(idx, function(a, b) return a > b end)
    local last
    for _, i in ipairs(idx) do
      if i ~= last then chk(i); table.remove(data, i) end
      last = i
    end
  end
  function methods.deleterange(a, b)
    if a > b then a, b = b, a end
    chk(a); chk(b)
    for i = b, a, -1 do table.remove(data, i) end
  end
  function methods.n() return #data end
  local function index(k)
    if type(k) == "number" then chk(k); return copy_line(data[k]) end
    return methods[k]
  end
  local function newindex(k, v)
    if type(k) ~= "number" then error("Cannot set non-numeric keys on subtitle file", 2) end
    if v == nil then
      chk(k); table.remove(data, k)
    else
      check_line(v)
      if k == #data + 1 then data[k] = copy_line(v) else chk(k); data[k] = copy_line(v) end
    end
  end
  return index, newindex, function() return #data end
end

-- ipairs/pairs that understand the subtitle proxy
local raw_ipairs = ipairs
function ipairs(t)
  if type(t) == "userdata" then
    local i = 0
    return function() i = i + 1; if i <= #t then return i, t[i] end end, t, 0
  end
  return raw_ipairs(t)
end

-- include / require support ---------------------------------------------
local loaded_includes = {}
function include(name)
  local src, chunkname = host.find_include(name)
  if not src then error("Lua include not found: " .. tostring(name), 2) end
  local f, err = loadstring(src, "@" .. chunkname)
  if not f then error(err, 2) end
  return f()
end
