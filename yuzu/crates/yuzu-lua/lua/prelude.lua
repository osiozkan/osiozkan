-- Builds the `aegisub` global from host primitives (__y) and registers library modules.
local Y = __y
aegisub = {}
aegisub.lua_automation_version = 4
aegisub.util = require("aegisub.util")
aegisub.re = require("re")

aegisub.debug = {}
local function fmt(...)
  local n = select("#", ...)
  if n == 0 then return "" end
  local ok, r = pcall(string.format, ...)
  if ok then return r end
  return tostring((...))
end
function aegisub.debug.out(a, ...)
  if type(a) == "number" then
    Y.log(a, fmt(...))
  else
    Y.log(3, fmt(a, ...))
  end
end
aegisub.log = aegisub.debug.out
aegisub.debug.warn = aegisub.debug.out
function aegisub.gettext(s) return s end
aegisub.cancel = function() error("__yuzu_cancel__", 0) end

aegisub.progress = {
  set = function(v) Y.progress("set", v) end,
  task = function(s) Y.progress("task", tostring(s)) end,
  title = function(s) Y.progress("title", tostring(s)) end,
  is_cancelled = function() return Y.is_cancelled() end,
}

aegisub.dialog = {
  display = function(dlg, buttons, ids) return Y.dialog(dlg, buttons, ids) end,
  open = function(...) return Y.file_dialog("open", ...) end,
  save = function(...) return Y.file_dialog("save", ...) end,
}

aegisub.register_macro = function(name, desc, fn, validate, active) Y.register_macro(name, desc, fn, validate, active) end
aegisub.register_filter = function(name, desc, prio, fn, cfg) Y.register_filter(name, desc, prio, fn, cfg) end
aegisub.register_file_associations = function() end
aegisub.text_extents = function(style, text) return Y.text_extents(style, text) end
aegisub.frame_from_ms = function(ms) return Y.frame_from_ms(ms) end
aegisub.ms_from_frame = function(f) return Y.ms_from_frame(f) end
aegisub.video_size = function() return Y.video_size() end
aegisub.keyframes = function() return Y.keyframes() end
aegisub.project_properties = function() return Y.project_properties() end
aegisub.decode_path = function(p) return Y.decode_path(p) end
aegisub.get_audio_selection = function() return Y.audio_selection() end
aegisub.get_cursor_pos = function() return 0 end

__yuzu_upper = function(s) return Y.case(s, true) end
__yuzu_lower = function(s) return Y.case(s, false) end
__yuzu_clip_get = Y.clip_get
__yuzu_clip_set = Y.clip_set
__yuzu_fs = Y.fs

unicode = nil
