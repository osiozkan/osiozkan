-- SubForge Auto4 bootstrap. Runs once per script, inside the script's own Lua state.
-- `B` is the Python bridge table.
local B = ...

aegisub = aegisub or {}
aegisub.lua_automation_version = 4
aegisub.__macros = {}
aegisub.__filters = {}
local CANCEL = "__aegisub_cancel__"

local function fmt(...)
  local n = select("#", ...)
  if n == 0 then return "" end
  local f = ...
  if n == 1 then return tostring(f) end
  local ok, s = pcall(string.format, ...)
  if ok then return s end
  return tostring(f)
end

function aegisub.cancel() error(CANCEL, 0) end

function aegisub.log(a, ...)
  local level, rest
  if type(a) == "number" then
    level = a
    B.log(level, fmt(...))
  else
    B.log(2, fmt(a, ...))
  end
end

aegisub.debug = { out = function(...) aegisub.log(...) end }

function aegisub.register_macro(name, desc, process, validate, is_active)
  assert(type(name) == "string" and type(process) == "function", "register_macro: bad arguments")
  table.insert(aegisub.__macros, {name = name, description = desc or "", process = process,
                                  validate = validate, is_active = is_active})
end

function aegisub.register_filter(name, desc, priority, process, config)
  assert(type(name) == "string" and type(process) == "function", "register_filter: bad arguments")
  table.insert(aegisub.__filters, {name = name, description = desc or "", priority = priority or 0,
                                   process = process, config = config})
end

aegisub.progress = {
  set = function(v) B.progress("set", v) end,
  task = function(...) B.progress("task", fmt(...)) end,
  title = function(...) B.progress("title", fmt(...)) end,
  is_cancelled = function() return B.progress_cancelled() end,
}

function aegisub.text_extents(style, text)
  local w, h, d, e = B.text_extents(style, text)
  return w, h, d, e
end

function aegisub.frame_from_ms(ms) return B.frame_from_ms(ms) end
function aegisub.ms_from_frame(f) return B.ms_from_frame(f) end
function aegisub.video_size() return B.video_size() end
function aegisub.keyframes() return B.keyframes() end
function aegisub.decode_path(p) return B.decode_path(p) end
function aegisub.project_properties() return B.project_properties() end
function aegisub.gettext(s) return s end

aegisub.clipboard = {
  get = function() return B.clipboard_get() end,
  set = function(s) return B.clipboard_set(s) end,
}

aegisub.dialog = {}
function aegisub.dialog.display(dialog, buttons, button_ids)
  local btn, res = B.dialog(dialog, buttons, button_ids)
  if btn == nil or btn == false then
    return false, res
  end
  return btn, res
end
function aegisub.dialog.open(title, default_file, default_dir, wildcards, allow_multiple, must_exist)
  return B.file_dialog("open", title, default_file, default_dir, wildcards, allow_multiple, must_exist)
end
function aegisub.dialog.save(title, default_file, default_dir, wildcards, prompt_overwrite)
  return B.file_dialog("save", title, default_file, default_dir, wildcards, prompt_overwrite)
end

-- ----- include / require ---------------------------------------------------
local include_dirs = B.include_dirs()
local sep = package.config:sub(1, 1)
local paths = {}
for _, d in ipairs(include_dirs) do
  table.insert(paths, d .. sep .. "?.lua")
end
package.path = table.concat(paths, ";") .. ";" .. package.path

function include(name)
  local fname = name
  if not fname:match("%.lua$") then fname = fname .. ".lua" end
  local f
  for _, d in ipairs(include_dirs) do
    local p = d .. sep .. fname
    local fh = io.open(p, "rb")
    if fh then fh:close(); f = p; break end
  end
  if not f then error("Lua include not found: " .. name, 2) end
  local chunk, err = loadfile(f)
  if not chunk then error(err, 2) end
  return chunk()
end

-- native-backed modules
package.preload["aegisub.re"] = function() return B.re_module() end
package.preload["re"] = function() return require("aegisub.re") end
package.preload["unicode"] = function() return B.unicode_module() end
package.preload["aegisub.unicode"] = function() return require("unicode") end
package.preload["aegisub.clipboard"] = function() return aegisub.clipboard end
package.preload["aegisub.lfs"] = function() return B.lfs_module() end
package.preload["lfs"] = function() return require("aegisub.lfs") end

-- ----- subtitle collection proxy (userdata so that #subs works under LuaJIT) --
function __make_subs(P)
  local u = newproxy(true)
  local mt = getmetatable(u)
  local methods = {}
  local function strip(self, ...)
    if rawequal(self, u) then return ... end
    return self, ...
  end
  function methods.insert(...)
    return P.insert(strip(...))
  end
  function methods.append(...)
    return P.append(strip(...))
  end
  function methods.delete(...)
    return P.delete(strip(...))
  end
  function methods.deleterange(...)
    return P.deleterange(strip(...))
  end
  function methods.n() return P.count() end
  mt.__index = function(_, k)
    if type(k) == "number" then
      return P.get(k)
    elseif k == "n" then
      return P.count()
    end
    return methods[k]
  end
  mt.__newindex = function(_, k, v)
    if type(k) ~= "number" then error("cannot set non-numeric key on subtitles", 2) end
    if k == 0 then P.append(v) else P.set(k, v) end
  end
  mt.__len = function() return P.count() end
  mt.__tostring = function() return "subtitles" end
  return u
end

function __run_macro(m, P, sel, active, validate_only)
  local subs = __make_subs(P)
  if validate_only then
    if not m.validate then return true end
    local ok, msg = m.validate(subs, sel, active)
    return ok, msg
  end
  local r1, r2 = m.process(subs, sel, active)
  return r1, r2
end

function __run_filter(f, P, config)
  local subs = __make_subs(P)
  return f.process(subs, config)
end

package.preload["aegisub.util"] = function() return include("util.lua") end
package.preload["util"] = function() return require("aegisub.util") end
