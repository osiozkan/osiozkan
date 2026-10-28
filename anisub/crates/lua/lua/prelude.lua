-- Automation 4 runtime surface (aegisub.* API) for anisub.
aegisub = aegisub or {}
local A = aegisub
A.__macros, A.__filters, A.__log = {}, {}, {}
A.__fps = 24000 / 1001

script_name, script_description, script_author, script_version = nil, nil, nil, nil

function A.register_macro(name, description, process, validate, is_active)
  if type(name) ~= "string" or type(process) ~= "function" then
    error("register_macro: bad arguments", 2)
  end
  local sep = script_name and (script_name .. "/") or ""
  table.insert(A.__macros, { name = name, description = description or "", process = process,
    validate = validate, is_active = is_active })
end

function A.register_filter(name, description, priority, process, config)
  table.insert(A.__filters, { name = name, description = description or "", priority = priority,
    process = process, config = config })
end

local function out(level, msg, ...)
  if select("#", ...) > 0 then msg = string.format(msg, ...) end
  table.insert(A.__log, tostring(msg))
end
function A.log(a, b, ...)
  if type(a) == "number" then return out(a, b or "", ...) end
  return out(2, a, b, ...)
end
A.debug = { out = function(a, b, ...)
  if type(a) == "number" then return out(a, b or "", ...) end
  return out(2, a, b, ...)
end }
function A.__take_log() local s = table.concat(A.__log, "\n"); A.__log = {}; return s end

A.progress = {
  set = function() end, task = function() end, title = function() end,
  is_cancelled = function() return false end,
}
local CANCEL = {}
function A.cancel() error(CANCEL, 0) end
function A.gettext(s) return s end
function A.lua_supports(_) return false end

function A.frame_from_ms(ms) return math.floor(ms * A.__fps / 1000 + 0.5 - 0.5 + 1e-9) end
function A.ms_from_frame(f) return math.floor(f * 1000 / A.__fps + 0.5) end
function A.video_size() return nil end
function A.keyframes() return {} end
function A.project_properties() return {} end
function A.get_audio_selection() return nil end
function A.get_cursor_time() return nil end

function A.decode_path(p)
  p = p:gsub("^%?temp", os.getenv("TMPDIR") or "/tmp")
  p = p:gsub("^%?user", (os.getenv("HOME") or ".") .. "/.anisub")
  p = p:gsub("^%?data", ".")
  p = p:gsub("^%?script", ".")
  return p
end
function A.file_name(p) return (p:gsub("^.*[/\\]", "")) end

A.clipboard = { get = function() return A.__clip or "" end, set = function(s) A.__clip = s; return true end }
A.cancel_flag = CANCEL

-- Headless dialog: every control keeps its default `value`; first button is "pressed".
A.dialog = {
  display = function(cfg, buttons, ids)
    local res = {}
    for _, c in ipairs(cfg or {}) do
      if c.name then
        local v = c.value
        if v == nil then
          if c.class == "checkbox" then v = false
          elseif c.class == "dropdown" then v = c.items and c.items[1] or ""
          else v = "" end
        end
        res[c.name] = v
      end
    end
    local b = buttons and buttons[1]
    if b == nil then b = true end
    return b, res
  end,
  open = function() return nil end,
  save = function() return nil end,
}

-- Approximate font metrics (a proper font-based implementation is on the roadmap).
function A.text_extents(style, text)
  local fs = style.fontsize or 40
  local n = 0
  for _ in text:gmatch("[%z\1-\127\194-\244][\128-\191]*") do n = n + 1 end
  local w = n * fs * 0.5 * ((style.scale_x or 100) / 100) + (style.spacing or 0) * n
  local h = fs * (style.scale_y or 100) / 100
  return w, h, h * 0.2, h * 0.1
end

-- Runs a macro/filter by name. Returns { selection=, active=, cancelled= }.
function A.__run(name, subs, selected, active)
  local entry
  for _, m in ipairs(A.__macros) do if m.name == name then entry = m break end end
  if not entry then
    for _, f in ipairs(A.__filters) do if f.name == name then entry = f break end end
  end
  if not entry then error("no such macro/filter: " .. tostring(name), 0) end
  local ok, a, b = pcall(function()
    if entry.config ~= nil or entry.priority ~= nil then
      return entry.process(subs, {})
    end
    return entry.process(subs, selected, active)
  end)
  if not ok then
    if a == CANCEL then return { cancelled = true } end
    error(a, 0)
  end
  local r = {}
  if type(a) == "table" then r.selection = a end
  if type(b) == "number" then r.active = b end
  if type(a) == "number" then r.active = a end
  return r
end
