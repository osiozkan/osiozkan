-- Yozora: Aegisub Automation 4 uyumluluk katmanı.
-- Rust tarafı bu parçayı `native` tablosu ile çağırır.
local native = ...

local _G = _G
local string, table, math = string, table, math

aegisub = {}
aegisub.lua_automation_version = 4
aegisub.yozora = true

local registry = { macros = {}, current_script = nil }
local logbuf = {}
local opts = native.options  -- { fps, params, script_dir, subs_path, video_w, video_h, keyframes, ... }

---------------------------------------------------------------------------
-- Günlük
---------------------------------------------------------------------------
local function fmt(...)
  local n = select('#', ...)
  if n == 0 then return "" end
  local first = ...
  if n == 1 then return tostring(first) end
  local ok, res = pcall(string.format, ...)
  if ok then return res end
  local parts = {}
  for i = 1, n do parts[i] = tostring((select(i, ...))) end
  return table.concat(parts, " ")
end

local function out(...)
  local level = 1
  local n = select('#', ...)
  local first = ...
  if type(first) == "number" and n >= 1 and type((select(2, ...))) == "string" then
    level = first
    logbuf[#logbuf + 1] = { level, fmt(select(2, ...)) }
  else
    logbuf[#logbuf + 1] = { level, fmt(...) }
  end
end

aegisub.debug = { out = out }
aegisub.log = out
_G.print = function(...)
  local t = {}
  for i = 1, select('#', ...) do t[i] = tostring((select(i, ...))) end
  logbuf[#logbuf + 1] = { 3, table.concat(t, "\t") }
end

---------------------------------------------------------------------------
-- İptal / ilerleme
---------------------------------------------------------------------------
local CANCEL = setmetatable({}, { __tostring = function() return "yozora:cancel" end })
function aegisub.cancel() error(CANCEL, 0) end
aegisub.progress = {
  set = function(v) native.progress("set", v) end,
  task = function(...) native.progress("task", fmt(...)) end,
  title = function(...) native.progress("title", fmt(...)) end,
  is_cancelled = function() return false end,
}

---------------------------------------------------------------------------
-- Yol, kare/zaman, proje bilgisi
---------------------------------------------------------------------------
function aegisub.decode_path(p)
  local map = {
    ["?script"] = opts.script_dir_subs or ".",
    ["?data"] = opts.data_dir or ".",
    ["?user"] = opts.user_dir or ".",
    ["?local"] = opts.user_dir or ".",
    ["?temp"] = opts.temp_dir or "/tmp",
    ["?video"] = opts.video_dir or ".",
    ["?audio"] = opts.video_dir or ".",
    ["?dictionary"] = opts.data_dir or ".",
  }
  for k, v in pairs(map) do
    if p:sub(1, #k) == k then return v .. p:sub(#k + 1) end
  end
  return p
end

function aegisub.file_name() return opts.subs_name end

local fps = opts.fps
function aegisub.frame_from_ms(ms)
  if not fps then return nil end
  return math.floor(ms * fps / 1000 + 1e-6)
end
function aegisub.ms_from_frame(f)
  if not fps then return nil end
  return math.floor(f * 1000 / fps + 0.5)
end
function aegisub.video_size()
  if opts.video_w and opts.video_w > 0 then return opts.video_w, opts.video_h, opts.video_w / opts.video_h, 0 end
  return nil
end
function aegisub.keyframes()
  local k = {}
  for i, v in ipairs(opts.keyframes or {}) do k[i] = v end
  return k
end
function aegisub.project_properties()
  return {
    video_file = opts.video_file or "", audio_file = "", timecodes_file = "", keyframes_file = "",
    video_position = 0, video_zoom = 1, ar_mode = 0, ar_value = 0,
    automation_scripts = "", export_filters = "",
    script_resolution_width = 0, script_resolution_height = 0,
  }
end
function aegisub.gettext(s) return s end
function aegisub.text_extents(style, text)
  return native.text_extents(style, text)
end
aegisub.audio_closest_keyframe = nil

local clip = ""
aegisub.clipboard = { get = function() return clip end, set = function(s) clip = tostring(s) return true end }

---------------------------------------------------------------------------
-- Diyaloglar (başsız mod: değerler `params` ile geçersiz kılınabilir)
---------------------------------------------------------------------------
local function coerce(class, v, orig)
  if v == nil then return orig end
  if class == "checkbox" then
    local s = tostring(v):lower()
    return s == "1" or s == "true" or s == "yes" or s == "on" or s == "evet"
  elseif class == "intedit" or class == "slider" then return math.floor(tonumber(v) or orig or 0)
  elseif class == "floatedit" then return tonumber(v) or orig or 0
  end
  return v
end

aegisub.dialog = {}
function aegisub.dialog.display(config, buttons, ids)
  local res = {}
  for _, c in ipairs(config or {}) do
    local n = c.name
    if n then
      local default = c.value
      if default == nil then default = c.text end
      if c.class == "checkbox" and default == nil then default = false end
      if (c.class == "intedit" or c.class == "floatedit" or c.class == "slider") and default == nil then default = c.min or 0 end
      if c.class == "dropdown" and default == nil then default = (c.items and c.items[1]) or "" end
      if default == nil then default = "" end
      res[n] = coerce(c.class, opts.params[n], default)
    end
  end
  buttons = buttons or { "OK", "Cancel" }
  local ok_btn = (ids and ids.ok) or buttons[1]
  if opts.params["__button"] then ok_btn = opts.params["__button"] end
  return ok_btn, res
end
function aegisub.dialog.open(title, default_dir, default_file, wildcards, multiple, must_exist)
  return opts.params["__open"]
end
function aegisub.dialog.save(title, default_dir, default_file, wildcards, prompt_overwrite)
  return opts.params["__save"]
end

---------------------------------------------------------------------------
-- Makro / süzgeç kaydı
---------------------------------------------------------------------------
function aegisub.register_macro(name, description, process, validate, is_active)
  registry.macros[#registry.macros + 1] = { kind = "macro", name = name, description = description or "", process = process, validate = validate, is_active = is_active }
end
function aegisub.register_filter(name, description, priority, process, config)
  registry.macros[#registry.macros + 1] = { kind = "filter", name = name, description = description or "", priority = priority or 0, process = process, config = config }
end

---------------------------------------------------------------------------
-- include / require
---------------------------------------------------------------------------
local function file_exists(p)
  local f = io.open(p, "rb")
  if f then f:close() return true end
  return false
end

local embedded = native.embedded -- ad -> kaynak
local search_dirs = { opts.script_dir, opts.script_dir and (opts.script_dir .. "/include"), opts.script_dir and (opts.script_dir .. "/../include") }
for _, d in ipairs(opts.include_dirs or {}) do search_dirs[#search_dirs + 1] = d end

local included = {}
function _G.include(name)
  local base = name:gsub("\\", "/")
  local tried = {}
  for _, d in ipairs(search_dirs) do
    if d then
      local p = d .. "/" .. base
      tried[#tried + 1] = p
      if file_exists(p) then
        local chunk, err = loadfile(p)
        if not chunk then error(err, 2) end
        return chunk()
      end
    end
  end
  if file_exists(base) then
    local chunk, err = loadfile(base)
    if not chunk then error(err, 2) end
    return chunk()
  end
  local src = embedded[base] or embedded[base:match("[^/]+$") or base]
  if src then
    local chunk, err = loadstring(src, "=" .. base)
    if not chunk then error(err, 2) end
    return chunk()
  end
  error("Lua include not found: " .. name .. "\nAranan yerler:\n  " .. table.concat(tried, "\n  "), 2)
end

local preload = package.preload
local function lib_loader(file, key)
  return function()
    if not _G.__yozora_libs then _G.__yozora_libs = {} end
    local libs = _G.__yozora_libs
    if libs[file] == nil then libs[file] = true; _G.include(file) end
    return _G[key] or true
  end
end
preload["utils"] = function() _G.include("utils.lua"); return _G.util end
preload["aegisub.util"] = function() _G.include("utils.lua"); return _G.util end
preload["karaskel"] = function() _G.include("karaskel.lua"); return _G.karaskel end
preload["unicode"] = function() _G.include("unicode.lua"); return _G.unicode end
preload["aegisub.unicode"] = preload["unicode"]
preload["re"] = function() return _G.re end
preload["aegisub.re"] = preload["re"]
preload["aegisub.clipboard"] = function() return aegisub.clipboard end
preload["clipboard"] = preload["aegisub.clipboard"]
preload["aegisub"] = function() return aegisub end

-- Bu isimler Aegisub'da önceden yüklüdür.
_G.re = native.re
_G.unicode = nil -- unicode.lua yüklenince tanımlanır
_G.script_name = nil

---------------------------------------------------------------------------
-- `subs` nesnesi
---------------------------------------------------------------------------
local function shallow(t)
  local c = {}
  for k, v in pairs(t) do c[k] = v end
  return c
end

local function make_subs(lines)
  local p = newproxy(true)
  local mt = getmetatable(p)
  local methods = {}
  local function unself(a, ...)
    if a == p then return ... end
    return a, ...
  end
  function methods.insert(...)
    local idx, rest = nil, nil
    local args = { unself(...) }
    idx = args[1]
    for i = 2, #args do
      table.insert(lines, idx + (i - 2), shallow(args[i]))
    end
  end
  function methods.append(...)
    local args = { unself(...) }
    for i = 1, #args do lines[#lines + 1] = shallow(args[i]) end
  end
  function methods.delete(...)
    local args = { unself(...) }
    table.sort(args, function(a, b) return a > b end)
    for _, i in ipairs(args) do
      if i < 1 or i > #lines then error("Attempt to delete a line out of range", 2) end
      table.remove(lines, i)
    end
  end
  function methods.deleterange(...)
    local a, b = unself(...)
    if a < 1 or b > #lines or a > b then error("Attempt to delete a range out of range", 2) end
    for i = b, a, -1 do table.remove(lines, i) end
  end
  methods.delete_range = methods.deleterange
  function methods.ipairs(...)
    local i = 0
    return function()
      i = i + 1
      if lines[i] then return i, shallow(lines[i]) end
    end
  end
  mt.__index = function(_, k)
    if type(k) == "number" then
      local l = lines[k]
      if not l then error("Requested line " .. k .. " out of range", 2) end
      return shallow(l)
    elseif k == "n" then
      return #lines
    end
    return methods[k]
  end
  mt.__newindex = function(_, k, v)
    if type(k) ~= "number" then error("Invalid subs index", 2) end
    if k == 0 then
      if v == nil then return end
      lines[#lines + 1] = shallow(v)
    elseif v == nil then
      table.remove(lines, k)
    elseif k < 0 then
      table.insert(lines, -k, shallow(v))
    else
      if k > #lines then error("Requested line " .. k .. " out of range", 2) end
      lines[k] = shallow(v)
    end
  end
  mt.__len = function() return #lines end
  return p
end

---------------------------------------------------------------------------
-- Dışarıya açılan giriş noktaları
---------------------------------------------------------------------------
local function msghandler(e)
  if e == CANCEL then return e end
  if type(e) == "table" then return tostring(e) end
  return debug.traceback(tostring(e), 2)
end

local function pick(index) return registry.macros[index] end

local function run(index, lines, sel, active)
  local m = pick(index)
  if not m then return { ok = false, err = "makro bulunamadı" } end
  local subs = make_subs(lines)
  local ok, r1, r2
  if m.kind == "filter" then
    ok, r1 = xpcall(m.process, msghandler, subs, {})
    if m.priority == nil then end
  else
    ok, r1, r2 = xpcall(m.process, msghandler, subs, sel, active)
  end
  if not ok then
    if r1 == CANCEL then return { ok = false, cancelled = true } end
    return { ok = false, err = tostring(r1) }
  end
  local result = { ok = true, lines = lines, active = nil, sel = nil }
  if type(r1) == "table" then
    result.sel = r1
    if type(r2) == "number" then result.active = r2 end
  elseif type(r1) == "number" then
    result.active = r1
  end
  return result
end

local function validate(index, lines, sel, active)
  local m = pick(index)
  if not m then return false end
  if not m.validate then return true end
  local subs = make_subs(lines)
  local ok, res = pcall(m.validate, subs, sel, active)
  if not ok then return false end
  return res and true or false
end

local function describe()
  local list = {}
  for i, m in ipairs(registry.macros) do
    list[i] = { m.kind, m.name, m.description, m.priority or 0 }
  end
  return list
end

local function take_log()
  local l = logbuf
  logbuf = {}
  return l
end

return { run = run, validate = validate, describe = describe, take_log = take_log }
