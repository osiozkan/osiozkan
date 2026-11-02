-- osisub: Aegisub Automation 4 compatibility prelude (runs after native bindings are installed)
local aegisub = aegisub

local function fmt(...)
  local n = select("#", ...)
  if n <= 1 then return tostring((...)) end
  local ok, s = pcall(string.format, ...)
  if ok then return s end
  return tostring((...))
end

function aegisub.log(...)
  local first = ...
  if type(first) == "number" then
    -- first argument is a trace level
    local rest = { select(2, ...) }
    if #rest == 0 then return end
    __osisub_log(fmt(unpack(rest)))
  else
    __osisub_log(fmt(...))
  end
end
aegisub.debug = {
  out = function(...) aegisub.log(...) end,
}
aegisub.log_level = 3
function aegisub.gettext(s) return s end
function aegisub.decode_path(p) return p end
function aegisub.file_name() return __osisub_script_file or "" end
function aegisub.cancel() error("__osisub_cancel__", 0) end
function aegisub.lua_supported_encoding() return true end
aegisub.progress = {
  set = function() end, task = function() end, title = function() end,
  is_cancelled = function() return false end,
}
aegisub.clipboard = { get = function() return nil end, set = function() return false end }
function aegisub.get_audio_selection() return nil end

-- Headless dialog: returns each control's default value, as if the user pressed the first button.
function aegisub.dialog.display(dialog, buttons, button_ids)
  local result = {}
  for _, c in ipairs(dialog or {}) do
    if c.name then
      local v = c.value
      if c.class == "checkbox" then v = v and true or false
      elseif c.class == "dropdown" then if v == nil then v = c.items and c.items[1] or "" end
      elseif c.class == "intedit" or c.class == "floatedit" or c.class == "coloralpha" or c.class == "color" then v = v or c.min or 0
      elseif c.class == "edit" or c.class == "textbox" or c.class == "label" then v = v or c.text or "" end
      result[c.name] = v
    end
  end
  if __osisub_dialog_values then
    for k, v in pairs(__osisub_dialog_values) do result[k] = v end
  end
  if buttons == nil then return true, result end
  local ok = (button_ids and button_ids.ok) or buttons[1]
  return ok, result
end
function aegisub.dialog.open() return nil end
function aegisub.dialog.save() return nil end

-- Registered file-based modules ---------------------------------------------
package.preload["aegisub.util"] = function() return __osisub_embedded("util.lua") end
package.preload["util"] = package.preload["aegisub.util"]
package.preload["unicode"] = function() return __osisub_embedded("unicode.lua") end
package.preload["aegisub.unicode"] = package.preload["unicode"]
package.preload["utf8"] = function() return __osisub_embedded("utf8.lua") end
package.preload["karaskel"] = function() return __osisub_embedded("karaskel.lua") end

util = require("aegisub.util")
unicode = require("unicode")

local loaded_includes = {}
function include(name)
  local base = name:gsub("%.lua$", "")
  if base == "utils" then return util end
  if base == "karaskel" then
    karaskel = require("karaskel")
    return karaskel
  end
  if base == "unicode" then return unicode end
  if base == "utf8" then utf8 = require("utf8"); return utf8 end
  if loaded_includes[name] then return end
  -- user include: script dir, then include dirs
  for _, dir in ipairs(__osisub_include_dirs or {}) do
    local path = dir .. "/" .. name
    local f = io.open(path, "rb")
    if f then
      f:close()
      loaded_includes[name] = true
      return dofile(path)
    end
  end
  error("include: cannot find '" .. name .. "'", 2)
end
