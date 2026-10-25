-- OsiSub Aegisub-compatible automation prelude (Lua 5.3 via fengari)
local host = __host

------------------------------------------------------------------ JSON
local json = {}
do
  local esc = { ['"'] = '\\"', ['\\'] = '\\\\', ['\b'] = '\\b', ['\f'] = '\\f', ['\n'] = '\\n', ['\r'] = '\\r', ['\t'] = '\\t' }
  local function estr(s) return '"' .. s:gsub('[%c"\\]', function(c) return esc[c] or string.format('\\u%04x', c:byte()) end) .. '"' end
  local function enc(v, out)
    local t = type(v)
    if t == 'nil' then out[#out + 1] = 'null'
    elseif t == 'boolean' then out[#out + 1] = tostring(v)
    elseif t == 'number' then
      if v ~= v or v == math.huge or v == -math.huge then out[#out + 1] = 'null'
      elseif math.type(v) == 'integer' then out[#out + 1] = tostring(v)
      elseif v == math.floor(v) and math.abs(v) < 1e15 then out[#out + 1] = string.format('%d', v)
      else out[#out + 1] = string.format('%.14g', v) end
    elseif t == 'string' then out[#out + 1] = estr(v)
    elseif t == 'table' then
      local n = 0; for _ in pairs(v) do n = n + 1 end
      if n == 0 then out[#out + 1] = '[]'
      elseif n == #v then
        out[#out + 1] = '['
        for i = 1, n do if i > 1 then out[#out + 1] = ',' end; enc(v[i], out) end
        out[#out + 1] = ']'
      else
        out[#out + 1] = '{'; local first = true
        for k, x in pairs(v) do
          if type(x) ~= 'function' and type(x) ~= 'userdata' then
            if not first then out[#out + 1] = ',' end; first = false
            out[#out + 1] = estr(tostring(k)) .. ':'; enc(x, out)
          end
        end
        out[#out + 1] = '}'
      end
    else out[#out + 1] = 'null' end
  end
  function json.encode(v) local o = {}; enc(v, o); return table.concat(o) end
  local function skip(s, i) return s:find('[^ \t\r\n]', i) or #s + 1 end
  local dec
  local function dstr(s, i)
    local out, j = {}, i + 1
    while true do
      local a = s:find('["\\]', j)
      if not a then error('unterminated string') end
      out[#out + 1] = s:sub(j, a - 1)
      if s:sub(a, a) == '"' then return table.concat(out), a + 1 end
      local c = s:sub(a + 1, a + 1)
      local m = { b = '\b', f = '\f', n = '\n', r = '\r', t = '\t', ['/'] = '/', ['\\'] = '\\', ['"'] = '"' }
      if c == 'u' then
        local cp = tonumber(s:sub(a + 2, a + 5), 16); j = a + 6
        if cp >= 0xD800 and cp < 0xDC00 and s:sub(j, j + 1) == '\\u' then
          local lo = tonumber(s:sub(j + 2, j + 5), 16); cp = 0x10000 + ((cp - 0xD800) << 10) + (lo - 0xDC00); j = j + 6
        end
        out[#out + 1] = utf8.char(cp)
      else out[#out + 1] = m[c]; j = a + 2 end
    end
  end
  function dec(s, i)
    i = skip(s, i); local c = s:sub(i, i)
    if c == '{' then
      local t = {}; i = skip(s, i + 1)
      if s:sub(i, i) == '}' then return t, i + 1 end
      while true do
        local k; k, i = dstr(s, skip(s, i)); i = skip(s, i) + 1
        t[k], i = dec(s, i); i = skip(s, i)
        local d = s:sub(i, i); i = i + 1
        if d == '}' then return t, i end
      end
    elseif c == '[' then
      local t = {}; i = skip(s, i + 1)
      if s:sub(i, i) == ']' then return t, i + 1 end
      while true do
        t[#t + 1], i = dec(s, i); i = skip(s, i)
        local d = s:sub(i, i); i = i + 1
        if d == ']' then return t, i end
      end
    elseif c == '"' then return dstr(s, i)
    elseif s:sub(i, i + 3) == 'true' then return true, i + 4
    elseif s:sub(i, i + 4) == 'false' then return false, i + 5
    elseif s:sub(i, i + 3) == 'null' then return nil, i + 4
    else
      local num = s:match('^-?%d+%.?%d*[eE]?[+-]?%d*', i)
      return math.tointeger(tonumber(num)) or tonumber(num), i + #num
    end
  end
  function json.decode(s) return (dec(s, 1)) end
end
_G.__json = json
local function H(name, ...) -- call host with JSON args, JSON result
  local r = host(name, json.encode({ ... }))
  if r == '' then return nil end
  return json.decode(r)
end

------------------------------------------------------------------ module loader
local libs = {}
local function load_virtual(name)
  local src = host('lib', name)
  if src ~= '' then return src end
end
package.path = ''
table.insert(package.searchers, 2, function(name)
  local fname = name:gsub('%.lua$', '')
  local src = load_virtual(fname) or load_virtual(fname:gsub('%.', '/'))
  if src then
    local f, e = load(src, '=' .. fname, 't')
    if not f then error(e) end
    return f
  end
  local fsrc = host('readfile', name:gsub('%.', '/') .. '.lua')
  if fsrc ~= '' then return (load(fsrc, '=' .. name, 't')) end
  return '\n\tno module ' .. name
end)

function include(name)
  local src = load_virtual((name:gsub('%.lua$', ''))) or (function() local s = host('readfile', name); if s ~= '' then return s end end)()
  if not src then error('Lua include not found: ' .. name) end
  local f, e = load(src, '=' .. name, 't', _ENV)
  if not f then error(e) end
  return f()
end

------------------------------------------------------------------ aegisub API
aegisub = { lua_automation_version = 4 }
local macros, filters, validators = {}, {}, {}
aegisub.__macros, aegisub.__filters = macros, filters

function aegisub.register_macro(name, desc, fn, validate, isactive)
  macros[#macros + 1] = { name = name, description = desc or '', fn = fn, validate = validate, is_active = isactive }
end
function aegisub.register_filter(name, desc, prio, fn, cfg)
  filters[#filters + 1] = { name = name, description = desc or '', priority = prio or 50, fn = fn, config = cfg }
end
function aegisub.gettext(s) return s end
_G.tr = aegisub.gettext
aegisub.cancel = function() error({ __cancel = true }, 0) end
function aegisub.progress_set(p) H('progress', p) end
function aegisub.progress_task(t) H('task', t) end
function aegisub.progress_title(t) H('title', t) end
function aegisub.debug_cancelled() return false end
aegisub.progress = { set = aegisub.progress_set, task = aegisub.progress_task, title = aegisub.progress_title, is_cancelled = function() return false end }
local function fmtmsg(...)
  local n = select('#', ...)
  if n >= 1 then
    local ok, s = pcall(string.format, ...)
    if ok then return s end
    return tostring((...))
  end
  return ''
end
aegisub.debug = {
  out = function(a, ...)
    if type(a) == 'number' then H('log', fmtmsg(...)) else H('log', fmtmsg(a, ...)) end
  end,
}
aegisub.log = function(a, ...) if type(a) == 'number' then H('log', fmtmsg(...)) else H('log', fmtmsg(a, ...)) end end
function aegisub.text_extents(style, text)
  local r = H('text_extents', style, text)
  return r[1], r[2], r[3], r[4]
end
function aegisub.frame_from_ms(ms) return H('frame_from_ms', ms) end
function aegisub.ms_from_frame(f) return H('ms_from_frame', f) end
function aegisub.video_size()
  local r = H('video_size'); if not r then return nil end
  return r[1], r[2], r[3], r[4]
end
function aegisub.project_properties() return H('project_properties') or {} end
function aegisub.decode_path(p) return H('decode_path', p) or p end
function aegisub.file_name() return H('file_name') or '' end
function aegisub.get_audio_selection() return nil end
function aegisub.get_frame() return nil end
function aegisub.keyframes() return H('keyframes') or {} end
function aegisub.clipboard_get() return H('clipboard_get') end
aegisub.clipboard = { get = aegisub.clipboard_get, set = function(s) H('clipboard_set', s); return true end }
aegisub.dialog = {}
function aegisub.dialog.display(dialog, buttons, ids)
  local default_btns = buttons == nil
  buttons = buttons or { 'OK', 'Cancel' }
  local resp = coroutine.yield({ kind = 'dialog', dialog = dialog, buttons = buttons, ids = ids })
  local label = (not resp.cancelled) and resp.button or nil
  local ok_label, cancel_label = buttons[1], buttons[#buttons]
  if ids then
    if ids.cancel and (label == nil) then label = ids.cancel end
  end
  local vals = resp.values or {}
  if default_btns then return label == 'OK', vals end
  if label == nil then
    return (ids and ids.cancel) or cancel_label or false, vals
  end
  return label, vals
end
aegisub.dialog.open = function(...) return H('dialog_open', ...) end
aegisub.dialog.save = function(...) return H('dialog_save', ...) end
aegisub.io = {}

-- Unicode / regex libs injected as modules
package.preload['aegisub.util'] = function() return require 'utils' end

------------------------------------------------------------------ subs object
local STYLE_BASE = { class = 'style', section = 'V4+ Styles' }
local function prep(l)
  if l.class == 'dialogue' then
    l.margin_b = l.margin_t; l.extra = l.extra or {}
    l.raw = nil
  elseif l.class == 'style' then l.margin_b = l.margin_t; l.relative_to = l.relative_to or 2 end
  return l
end

local Subs = {}
local function new_subs(lines)
  local data = {}
  for i, l in ipairs(lines) do data[i] = prep(l) end
  local api = {}
  -- Aegisub calls these with dot syntax: subs.insert(i, line)
  function api.insert(i, ...) for k, v in ipairs({ ... }) do table.insert(data, i + k - 1, v) end end
  function api.append(...) for _, v in ipairs({ ... }) do data[#data + 1] = v end end
  function api.delete(...)
    local idx = { ... }; if type(idx[1]) == 'table' then idx = idx[1] end
    table.sort(idx, function(a, b) return a > b end)
    for _, i in ipairs(idx) do table.remove(data, i) end
  end
  function api.deleterange(a, b) for i = b, a, -1 do table.remove(data, i) end end
  function api.get_script_info() return {} end
  local s = {}
  return setmetatable(s, {
    __index = function(_, k)
      if type(k) == 'number' then
        local v = data[k]
        if v == nil then error('Requested out-of-range line from subtitle file: ' .. tostring(k), 2) end
        return v
      end
      if k == 'n' then return #data end
      return api[k]
    end,
    __newindex = function(_, k, v)
      if type(k) ~= 'number' or k < 1 or k > #data + 1 then error('Attempt to set out-of-range line', 2) end
      data[k] = v
    end,
    __len = function() return #data end,
    __pairs = function() return ipairs(data) end,
    __data = data,
  })
end
local function subs_data(s) return getmetatable(s).__data end

------------------------------------------------------------------ driver
local running, current
local ASS_STYLE_NUMS = {}

local function lines_out(subs)
  local d = subs_data(subs); local out = {}
  for i, l in ipairs(d) do
    local c = {}
    for k, v in pairs(l) do if k ~= 'extra' and k ~= 'margin_b' and k ~= 'relative_to' then c[k] = v end end
    if l.class == 'style' and l.margin_b and l.margin_b ~= l.margin_t and l.margin_t == nil then c.margin_t = l.margin_b end
    out[i] = c
  end
  return out
end

local function step(co, ...)
  local ok, a = coroutine.resume(co, ...)
  if not ok then
    if type(a) == 'table' and a.__cancel then return { status = 'cancelled' } end
    return { status = 'error', message = type(a) == 'table' and tostring(a.message or a[1] or a) or tostring(a) }
  end
  if coroutine.status(co) == 'suspended' then return { status = 'dialog', request = a } end
  local subs, sel, act = current.subs, a, nil
  local res = { status = 'done', lines = lines_out(subs) }
  if type(a) == 'table' then
    if type(a[1]) == 'table' then res.selection = a[1]; res.active = a[2] else res.selection = a; end
  end
  return res
end

function __start(kind, idx, linesJson, selJson, active)
  local lines = json.decode(linesJson); local sel = json.decode(selJson)
  local subs = new_subs(lines)
  current = { subs = subs }
  local item = (kind == 'filter') and filters[idx] or macros[idx]
  local fn
  if kind == 'filter' then
    fn = function() item.fn(subs, {}); return sel end
  else
    fn = function() return item.fn(subs, sel, active) end
  end
  running = coroutine.create(fn)
  return json.encode(step(running))
end
function __resume(respJson)
  return json.encode(step(running, json.decode(respJson)))
end
function __validate(idx, linesJson, selJson, active)
  local item = macros[idx]
  if not item.validate then return json.encode({ ok = true }) end
  local subs = new_subs(json.decode(linesJson))
  local ok, r, msg = pcall(item.validate, subs, json.decode(selJson), active)
  return json.encode({ ok = ok and r ~= false, message = msg })
end
function __info()
  local ms, fs = {}, {}
  for i, m in ipairs(macros) do ms[i] = { name = m.name, description = m.description } end
  for i, m in ipairs(filters) do fs[i] = { name = m.name, description = m.description } end
  return json.encode({ name = script_name, description = script_description, author = script_author, version = script_version, macros = ms, filters = fs })
end
require 'utils'
