-- OsiSub: Aegisub Lua API (automation v4) compatibility layer.
-- Python injects the global table `__host` with callbacks before this runs.
local H = __host
local floor = math.floor

aegisub = {}
aegisub.lua_automation_version = 4
aegisub.__macros, aegisub.__filters, aegisub.__exports = {}, {}, {}

function aegisub.register_macro(name, desc, fn, validate, is_active)
  table.insert(aegisub.__macros, {name = name, description = desc, fn = fn, validate = validate, is_active = is_active})
end
function aegisub.register_filter(name, desc, prio, fn, config)
  table.insert(aegisub.__filters, {name = name, description = desc, priority = prio, fn = fn, config = config})
end
function aegisub.gettext(s) return s end
function aegisub.cancel() error("__OSISUB_CANCEL__", 0) end

aegisub.progress = {
  set = function(v) H.progress(v) end,
  task = function(t) H.task(t) end,
  title = function(t) H.title(t) end,
  is_cancelled = function() return false end,
}
aegisub.debug = {
  out = function(a, b, ...)
    if type(a) == "number" then
      local ok, s = pcall(string.format, b or "", ...)
      H.log(a, ok and s or tostring(b))
    else
      local ok, s = pcall(string.format, a or "", b, ...)
      H.log(3, ok and s or tostring(a))
    end
  end,
}
aegisub.debug.out = aegisub.debug.out
debug_out = nil
function aegisub.log(...) return aegisub.debug.out(...) end

aegisub.dialog = {}
function aegisub.dialog.display(dialog, buttons, ids)
  -- headless default: first button, every control keeps its current value
  local res = H.dialog(dialog, buttons)
  if res then return res[1], res[2] end
  local out = {}
  for _, c in ipairs(dialog) do
    if c.name then
      if c.class == "checkbox" then out[c.name] = c.value or false
      elseif c.class == "dropdown" then out[c.name] = c.value or (c.items and c.items[1])
      elseif c.class == "edit" or c.class == "textbox" or c.class == "label" then out[c.name] = c.text or c.value or ""
      else out[c.name] = c.value end
    end
  end
  local b = buttons and buttons[1]
  if b == nil then b = true end
  return b, out
end
function aegisub.dialog.open() return H.file_dialog and H.file_dialog("open") or nil end
function aegisub.dialog.save() return H.file_dialog and H.file_dialog("save") or nil end

function aegisub.text_extents(style, text)
  local w, h, d, e = H.text_extents(style, text)
  return w, h, d, e
end
function aegisub.frame_from_ms(ms) return H.frame_from_ms(ms) end
function aegisub.ms_from_frame(f) return H.ms_from_frame(f) end
function aegisub.video_size() return H.video_size() end
function aegisub.keyframes() return {} end
function aegisub.decode_path(p) return H.decode_path(p) end
function aegisub.project_properties() return H.project_properties() end
function aegisub.get_audio_selection() return 0, 0 end
function aegisub.set_undo_point(desc) end

-- Aegisub also exposes the legacy global
function include(name)
  for _, dir in ipairs(H.include_dirs) do
    local f = io.open(dir .. "/" .. name, "r")
    if f then f:close(); return dofile(dir .. "/" .. name) end
  end
  error("include: cannot find " .. name, 2)
end

-- Lua 5.1/LuaJIT already has unpack, loadstring, setfenv. Extra aliases:
table.pack = table.pack or function(...) return {n = select("#", ...), ...} end
table.unpack = table.unpack or unpack

-- unicode / re / util modules
unicode = {}
function unicode.chars(s) return s:gmatch("[%z\1-\127\194-\244][\128-\191]*") end
function unicode.len(s) return H.u_len(s) end
function unicode.codepoint(c) return H.u_codepoint(c) end
function unicode.codepoints(s) local t = {} for c in unicode.chars(s) do t[#t + 1] = H.u_codepoint(c) end return t end
function unicode.to_upper_case(s) return H.u_upper(s) end
function unicode.to_lower_case(s) return H.u_lower(s) end
function unicode.to_fold_case(s) return H.u_lower(s) end
function unicode.charwidth(c) return 1 end
package.preload["unicode"] = function() return unicode end
package.preload["aegisub.unicode"] = package.preload["unicode"]

re = {}
local function wrap(m)
  if not m then return nil end
  local r = {}
  for i, x in ipairs(m) do r[i] = {str = x[1], first = x[2], last = x[3]} end
  return r
end
function re.find(s, p, flags) return wrap(H.re_find(s, p, flags or 0)) end
function re.match(s, p, flags) return wrap(H.re_match(s, p, flags or 0)) end
function re.sub(s, p, r, count, flags) return H.re_sub(s, p, r, count or 0, flags or 0) end
function re.gsub(s, p, r, count, flags) return H.re_sub(s, p, r, count or 0, flags or 0) end
function re.compile(p, flags)
  return {
    find = function(_, s) return re.find(s, p, flags) end,
    match = function(_, s) return re.match(s, p, flags) end,
    sub = function(_, s, r, c) return re.sub(s, p, r, c, flags) end,
    gsub = function(_, s, r, c) return re.sub(s, p, r, c, flags) end,
  }
end
re.ICASE, re.NOSUB, re.NO_MOD_M, re.MOD_S, re.MOD_X, re.NOT_BOL = 1, 2, 4, 8, 16, 32
package.preload["re"] = function() return re end
package.preload["aegisub.re"] = package.preload["re"]

local clip = {}
function clip.get() return H.clip_get and H.clip_get() or "" end
function clip.set(s) if H.clip_set then H.clip_set(s) end return true end
package.preload["aegisub.clipboard"] = function() return clip end
aegisub.clipboard = clip

-- builds the `subs` object handed to macros
function __make_subs(lines)
  local subs = lines
  local meta = {__index = function(t, k) if k == "n" then return #t end end}
  rawset(subs, "insert", function(i, ...)
    local items = {...}
    for k = #items, 1, -1 do table.insert(subs, i, items[k]) end
  end)
  rawset(subs, "append", function(...)
    for _, l in ipairs({...}) do subs[#subs + 1] = l end
  end)
  rawset(subs, "delete", function(...)
    local idx = {...}
    if type(idx[1]) == "table" then idx = idx[1] end
    table.sort(idx, function(a, b) return a > b end)
    for _, i in ipairs(idx) do table.remove(subs, i) end
  end)
  rawset(subs, "deleterange", function(a, b) for i = b, a, -1 do table.remove(subs, i) end end)
  rawset(subs, "set_undo_point", function() end)
  setmetatable(subs, meta)
  return subs
end
