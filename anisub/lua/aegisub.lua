-- Aegisub automation API shim (Lua side). Native pieces are provided by the host in `__host`.
aegisub = aegisub or {}
local H = __host

__macros = {}
__filters = {}

function aegisub.register_macro(name, description, fn, validate, is_active)
  table.insert(__macros, {name = name, description = description, fn = fn, validate = validate, is_active = is_active})
end
function aegisub.register_filter(name, description, priority, fn, config)
  table.insert(__filters, {name = name, description = description, priority = priority, fn = fn, config = config})
end

local function fmt(...)
  local a = {...}
  if type(a[1]) == "number" and type(a[2]) == "string" then table.remove(a, 1) end
  if #a == 0 then return "" end
  local ok, s = pcall(string.format, unpack(a))
  if ok then return s end
  return tostring(a[1])
end
function aegisub.log(...) H.log(fmt(...)) end
aegisub.debug = {out = function(...) H.log(fmt(...)) end}

aegisub.progress = {
  set = function(p) H.progress(p) end,
  task = function(t) end,
  title = function(t) end,
  is_cancelled = function() return false end,
}
function aegisub.cancel() error("\0aegisub_cancel", 0) end
function aegisub.gettext(s) return s end
function aegisub.lua_supports_utf8() return true end
function aegisub.decode_path(p) return H.decode_path(p) end
function aegisub.text_extents(style, text) return H.text_extents(style, text) end

-- Timing/frame helpers. fps comes from the host (default 23.976).
local function fps() return H.fps() end
function aegisub.frame_from_ms(ms)
  local f = fps(); if not f or f <= 0 then return nil end
  return math.floor(ms * f / 1000 + 0.0000001)
end
function aegisub.ms_from_frame(fr)
  local f = fps(); if not f or f <= 0 then return nil end
  return math.floor(fr * 1000 / f + 0.5)
end
function aegisub.video_size() local x, y = H.video_size(); return x, y, 1 end
function aegisub.keyframes() return {} end
function aegisub.project_properties() return {video_file = "", audio_file = "", keyframes_file = "", timecodes_file = "", automation_scripts = "", video_position = 0, video_zoom = 1} end

aegisub.dialog = aegisub.dialog or {}
-- Headless dialog: returns defaults (checked controls, default values) and the first button.
function aegisub.dialog.display(dialog, buttons, ids)
  local result = {}
  for _, c in ipairs(dialog) do
    if c.name then
      if c.class == "checkbox" then result[c.name] = c.value or false
      elseif c.class == "dropdown" then result[c.name] = c.value or (c.items and c.items[1])
      else result[c.name] = c.value end
    end
  end
  buttons = buttons or {"OK"}
  H.log("[dialog] auto-accepted with defaults")
  return buttons[1] or "OK", result
end
function aegisub.dialog.open() return nil end
function aegisub.dialog.save() return nil end
