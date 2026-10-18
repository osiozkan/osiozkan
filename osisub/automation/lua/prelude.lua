-- Osisub automation prelude: Aegisub Automation 4 (Lua) compatible runtime.
-- Python host functions are reachable through the global __host table.
local host = __host
local _G = _G

local function fmt(f, ...)
  if select("#", ...) == 0 then return tostring(f) end
  local ok, r = pcall(string.format, f, ...)
  if ok then return r end
  return tostring(f)
end

----------------------------------------------------------------------
-- subtitle container (userdata so that '#subs' and index metamethods work)
----------------------------------------------------------------------
local function shallow(t)
  local c = {}
  for k, v in pairs(t) do c[k] = v end
  return c
end

local raw_ipairs = ipairs
ipairs = function(t)
  local m = getmetatable(t)
  if type(t) == "userdata" and m and m.__ipairs then return m.__ipairs(t) end
  return raw_ipairs(t)
end

local function make_subs(lines, selected_ids)
  local proxy = newproxy(true)
  local mt = getmetatable(proxy)
  local methods = {}

  local function chk(i, lo, hi)
    if type(i) ~= "number" or i ~= math.floor(i) or i < lo or i > hi then
      error(string.format("Attempt to access subtitle line %s, but only 1-%d exist", tostring(i), #lines), 3)
    end
  end

  local function norm(l)
    if type(l) ~= "table" then error("Attempt to insert non-table value as subtitle line", 3) end
    local c = shallow(l)
    if c.class == "dialogue" then
      c.section = c.section or "[Events]"
      c.comment = c.comment or false
      c.layer = c.layer or 0
      c.start_time = c.start_time or 0
      c.end_time = c.end_time or 0
      c.style = c.style or "Default"
      c.actor = c.actor or ""
      c.margin_l = c.margin_l or 0
      c.margin_r = c.margin_r or 0
      c.margin_t = c.margin_t or c.margin_v or 0
      c.margin_b = c.margin_b or c.margin_t
      c.effect = c.effect or ""
      c.text = c.text or ""
      c.extra = c.extra or {}
    end
    return c
  end

  function methods.insert(i, ...)
    local n = select("#", ...)
    chk(i, 1, #lines + 1)
    for k = 1, n do
      local l = norm((select(k, ...)))
      table.insert(lines, i + k - 1, l)
    end
  end
  function methods.append(...)
    for k = 1, select("#", ...) do table.insert(lines, norm((select(k, ...)))) end
  end
  function methods.delete(...)
    local idx = {...}
    table.sort(idx, function(a, b) return a > b end)
    for _, i in ipairs(idx) do chk(i, 1, #lines); table.remove(lines, i) end
  end
  function methods.deleterange(a, b)
    chk(a, 1, #lines); chk(b, 1, #lines)
    if a > b then a, b = b, a end
    for i = b, a, -1 do table.remove(lines, i) end
  end
  function methods.script_resolution()
    local x, y = host.script_resolution()
    return x, y
  end

  mt.__len = function() return #lines end
  mt.__index = function(_, k)
    if type(k) == "number" then
      if k < 0 then error("Attempt to read subtitle line at negative index", 2) end
      chk(k, 1, #lines)
      local l = shallow(lines[k])
      if l.extra then l.extra = shallow(l.extra) end
      return l
    elseif k == "n" then
      return #lines
    end
    return methods[k]
  end
  mt.__newindex = function(_, k, v)
    if type(k) ~= "number" then error("Cannot set non-numeric subtitle index", 2) end
    if k < 0 then
      -- negative index: insert before line -k
      local at = -k
      chk(at, 1, #lines + 1)
      table.insert(lines, at, norm(v))
    elseif k == 0 then
      table.insert(lines, #lines + 1, norm(v))
    else
      chk(k, 1, #lines)
      if v == nil then table.remove(lines, k) else lines[k] = norm(v) end
    end
  end
  mt.__ipairs = function()
    local i = 0
    return function() i = i + 1; if i <= #lines then return i, mt.__index(nil, i) end end
  end
  mt.__tostring = function() return "SubtitleFile" end
  return proxy
end

----------------------------------------------------------------------
-- unicode helpers
----------------------------------------------------------------------
local unicode = {}
local function utf8len(c)
  local b = c:byte(1)
  if not b then return 0 end
  if b < 0x80 then return 1 elseif b >= 0xF0 then return 4 elseif b >= 0xE0 then return 3 elseif b >= 0xC0 then return 2 end
  return 1
end
function unicode.charwidth(s, i)
  i = i or 1
  local b = s:byte(i)
  if not b then return 0 end
  return utf8len(s:sub(i, i))
end
function unicode.chars(s)
  local i = 1
  return function()
    if i > #s then return nil end
    local n = utf8len(s:sub(i, i))
    local c = s:sub(i, i + n - 1)
    i = i + n
    return c
  end
end
function unicode.len(s) local n = 0 for _ in unicode.chars(s) do n = n + 1 end return n end
function unicode.codepoint(s)
  local b1, b2, b3, b4 = s:byte(1, 4)
  if not b1 then return nil end
  if b1 < 0x80 then return b1
  elseif b1 < 0xE0 then return (b1 - 0xC0) * 64 + (b2 - 0x80)
  elseif b1 < 0xF0 then return (b1 - 0xE0) * 4096 + (b2 - 0x80) * 64 + (b3 - 0x80)
  else return (b1 - 0xF0) * 262144 + (b2 - 0x80) * 4096 + (b3 - 0x80) * 64 + (b4 - 0x80) end
end
function unicode.to_upper(s) return host.str_upper(s) end
function unicode.to_lower(s) return host.str_lower(s) end
function unicode.to_fold_case(s) return host.str_lower(s) end

----------------------------------------------------------------------
-- regex module (Perl-like, backed by Python re)
----------------------------------------------------------------------
local re = { ICASE = 1, NOSUB = 2, NO_MOD_M = 4, MOD_S = 8, MOD_M = 16, NOT_BOL = 32, NOT_EOL = 64 }
local Regex = {}
Regex.__index = Regex
function re.compile(pat, flags)
  local id = host.re_compile(pat, flags or 0)
  return setmetatable({ id = id }, Regex)
end
local function wrap_matches(m)
  if not m then return nil end
  return m
end
function Regex:find(str) return host.re_find(self.id, str) end
function Regex:gfind(str)
  local res = host.re_find(self.id, str)
  return res
end
function Regex:match(str) return host.re_find(self.id, str, true) end
function Regex:sub(str, repl, max) return host.re_sub(self.id, str, repl, max or 0) end
function Regex:gsub(str, repl, max) return host.re_sub(self.id, str, repl, max or 0) end
function Regex:split(str) return host.re_split(self.id, str) end
function re.find(str, pat, flags) return re.compile(pat, flags):find(str) end
function re.match(str, pat, flags) return re.compile(pat, flags):match(str) end
function re.sub(str, pat, repl, max, flags) return re.compile(pat, flags):sub(str, repl, max) end
re.gsub = re.sub
function re.split(str, pat, flags) return re.compile(pat, flags):split(str) end

----------------------------------------------------------------------
-- utility module
----------------------------------------------------------------------
local util = {}
function util.copy(t)
  local c = {}
  for k, v in pairs(t) do c[k] = v end
  return c
end
function util.deep_copy(t, seen)
  if type(t) ~= "table" then return t end
  seen = seen or {}
  if seen[t] then return seen[t] end
  local c = {}
  seen[t] = c
  for k, v in pairs(t) do c[util.deep_copy(k, seen)] = util.deep_copy(v, seen) end
  return setmetatable(c, getmetatable(t))
end
function util.clamp(v, lo, hi) return math.min(math.max(v, lo), hi) end
function util.interpolate(pct, a, b)
  if pct <= 0 then return a elseif pct >= 1 then return b end
  return pct * (b - a) + a
end
function util.ass_alpha(a) return string.format("&H%02X&", a) end
function util.ass_color(r, g, b) return string.format("&H%02X%02X%02X&", b, g, r) end
function util.ass_style_color(r, g, b, a) return string.format("&H%02X%02X%02X%02X", a or 0, b, g, r) end
function util.extract_color(s)
  local a, b, g, r
  local hex = s:match("^&?[Hh](%x+)&?$") or s:match("^&[Hh](%x+)")
  if hex then
    if #hex <= 2 then return tonumber(hex, 16) end
    while #hex < 8 do hex = "0" .. hex end
    a, b, g, r = hex:match("^(%x%x)(%x%x)(%x%x)(%x%x)$")
    if a then return tonumber(r, 16), tonumber(g, 16), tonumber(b, 16), tonumber(a, 16) end
  end
  r, g, b = s:match("^#(%x%x)(%x%x)(%x%x)$")
  if r then return tonumber(r, 16), tonumber(g, 16), tonumber(b, 16), 0 end
  return 0, 0, 0, 0
end
function util.alpha_from_style(s) local _, _, _, a = util.extract_color(s) return util.ass_alpha(a) end
function util.color_from_style(s) local r, g, b = util.extract_color(s) return util.ass_color(r, g, b) end
function util.interpolate_color(pct, c1, c2)
  local r1, g1, b1 = util.extract_color(c1)
  local r2, g2, b2 = util.extract_color(c2)
  local f = function(a, b) return math.floor(util.interpolate(pct, a, b) + 0.5) end
  return util.ass_color(f(r1, r2), f(g1, g2), f(b1, b2))
end
function util.interpolate_alpha(pct, a1, a2)
  local x = util.extract_color(a1)
  local y = util.extract_color(a2)
  return util.ass_alpha(math.floor(util.interpolate(pct, x, y) + 0.5))
end
function util.HSV_to_RGB(H, S, V)
  H = (H % 360) / 60
  local i = math.floor(H)
  local f = H - i
  local p, q, t = V * (1 - S), V * (1 - S * f), V * (1 - S * (1 - f))
  local r, g, b
  if i == 0 then r, g, b = V, t, p elseif i == 1 then r, g, b = q, V, p
  elseif i == 2 then r, g, b = p, V, t elseif i == 3 then r, g, b = p, q, V
  elseif i == 4 then r, g, b = t, p, V else r, g, b = V, p, q end
  return math.floor(r * 255 + 0.5), math.floor(g * 255 + 0.5), math.floor(b * 255 + 0.5)
end
function util.RGB_to_HSV(r, g, b)
  r, g, b = r / 255, g / 255, b / 255
  local mx, mn = math.max(r, g, b), math.min(r, g, b)
  local d = mx - mn
  local h = 0
  if d ~= 0 then
    if mx == r then h = ((g - b) / d) % 6 elseif mx == g then h = (b - r) / d + 2 else h = (r - g) / d + 4 end
    h = h * 60
  end
  return h, (mx == 0) and 0 or d / mx, mx
end
function util.HSL_to_RGB(H, S, L)
  H = (H % 360) / 360
  local function hue(p, q, t)
    if t < 0 then t = t + 1 elseif t > 1 then t = t - 1 end
    if t < 1 / 6 then return p + (q - p) * 6 * t elseif t < 1 / 2 then return q
    elseif t < 2 / 3 then return p + (q - p) * (2 / 3 - t) * 6 end
    return p
  end
  local r, g, b
  if S == 0 then r, g, b = L, L, L else
    local q = (L < 0.5) and L * (1 + S) or L + S - L * S
    local p = 2 * L - q
    r, g, b = hue(p, q, H + 1 / 3), hue(p, q, H), hue(p, q, H - 1 / 3)
  end
  return math.floor(r * 255 + 0.5), math.floor(g * 255 + 0.5), math.floor(b * 255 + 0.5)
end
function util.RGB_to_HSL(r, g, b)
  r, g, b = r / 255, g / 255, b / 255
  local mx, mn = math.max(r, g, b), math.min(r, g, b)
  local l = (mx + mn) / 2
  if mx == mn then return 0, 0, l end
  local d = mx - mn
  local s = (l > 0.5) and d / (2 - mx - mn) or d / (mx + mn)
  local h
  if mx == r then h = (g - b) / d + ((g < b) and 6 or 0) elseif mx == g then h = (b - r) / d + 2 else h = (r - g) / d + 4 end
  return h * 60, s, l
end

-- global (legacy utils.lua) flavour of the same helpers
local function install_utils_globals(env)
  env.util = util
  for _, n in ipairs { "clamp", "interpolate", "interpolate_color", "interpolate_alpha", "ass_alpha", "ass_color",
    "ass_style_color", "extract_color", "alpha_from_style", "color_from_style", "HSV_to_RGB", "HSL_to_RGB",
    "RGB_to_HSV", "RGB_to_HSL" } do env[n] = util[n] end
end

-- string/table extensions defined by utils.lua (global, affects all scripts like Aegisub)
function table.copy(t) return util.copy(t) end
function table.copy_deep(t) return util.deep_copy(t) end
function string.trim(s) return (s:gsub("^%s+", ""):gsub("%s+$", "")) end
function string.headtail(s)
  local a, b, head, tail = s:find("^%s*(%S+)%s*(.*)$")
  if a then return head, tail end
  return "", ""
end
function string.words(s)
  return function()
    local a, b, w = s:find("^%s*(%S+)")
    if not a then return nil end
    s = s:sub(b + 1)
    return w
  end
end

----------------------------------------------------------------------
-- karaoke parsing (aegisub.parse_karaoke_data)
----------------------------------------------------------------------
local function parse_karaoke_data(line)
  local text = line.text
  local syls = {}
  local cur = { duration = 0, tag = "", text = "", text_stripped = "", start_time = 0, end_time = 0, line = line }
  local function flush(final)
    cur.end_time = cur.start_time + cur.duration
    syls[#syls + 1] = cur
  end
  local pos = 1
  local t = 0
  local function add_text(s)
    cur.text = cur.text .. s
    cur.text_stripped = cur.text_stripped .. s
  end
  while pos <= #text do
    local a, b = text:find("{[^}]*}", pos)
    if not a then add_text(text:sub(pos)); break end
    if a > pos then add_text(text:sub(pos, a - 1)) end
    local block = text:sub(a + 1, b - 1)
    -- split block on karaoke tags
    local rest = block
    local first = true
    local kept = ""
    local function emit_kept()
      if kept ~= "" then cur.text = cur.text .. "{" .. kept .. "}"; kept = "" end
    end
    local p = 1
    local seg_start = 1
    local any = false
    while true do
      local ka, kb, ktag, kval = rest:find("\\([kK][fo]?)(%-?%d+)", p)
      if not ka then break end
      any = true
      kept = kept .. rest:sub(seg_start, ka - 1)
      emit_kept()
      -- finish current syllable if it has any content or a previous k tag
      if #syls > 0 or cur.text ~= "" or cur.tag ~= "" or true then
        flush()
        t = t + cur.duration
      end
      cur = { duration = tonumber(kval) * 10, tag = (ktag == "K") and "kf" or ktag, text = "", text_stripped = "", start_time = t, end_time = t, line = line }
      seg_start = kb + 1
      p = kb + 1
    end
    kept = kept .. rest:sub(seg_start)
    if kept ~= "" then cur.text = cur.text .. "{" .. kept .. "}" end
    pos = b + 1
  end
  flush()
  -- if the first syllable (index 0) has no text and zero duration it is still kept as kara[0]
  local out = {}
  for i, s in ipairs(syls) do
    s.i = i - 1
    out[i - 1] = s
  end
  out.n = #syls - 1
  return out
end

----------------------------------------------------------------------
-- script environments
----------------------------------------------------------------------
local scripts = {}
local current_script
local include_dirs = {}
local loaded_includes = {}

local function find_file(name, script)
  local function try(p)
    local f = io.open(p, "rb")
    if f then f:close() return p end
  end
  if name:match("^/") or name:match("^%a:[\\/]") then return try(name) end
  if script and script.dir then
    local p = try(script.dir .. "/" .. name)
    if p then return p end
  end
  for _, d in ipairs(include_dirs) do
    local p = try(d .. "/" .. name)
    if p then return p end
  end
end

local function load_into(path, env)
  local f, err = io.open(path, "rb")
  if not f then error("Could not open " .. path, 3) end
  local src = f:read("*a")
  f:close()
  if src:sub(1, 3) == "\239\187\191" then src = src:sub(4) end
  local chunk, e = loadstring(src, "@" .. path)
  if not chunk then error(e, 0) end
  setfenv(chunk, env)
  return chunk
end

local builtin_includes = {}

local function make_aegisub(script)
  local A = {}
  A.register_macro = function(name, description, process, validate, is_active)
    script.macros[#script.macros + 1] = { name = name, description = description or "", process = process, validate = validate, is_active = is_active }
  end
  A.register_filter = function(name, description, priority, process, config)
    script.filters[#script.filters + 1] = { name = name, description = description or "", priority = priority or 0, process = process, config = config }
  end
  A.cancel = function() error({ __cancel = true }, 0) end
  A.gettext = function(s) return s end
  A.lua_automation_version = 4
  A.progress = {
    set = function(v) host.progress_set(v) end,
    task = function(...) host.progress_task(fmt(...)) end,
    title = function(...) host.progress_title(fmt(...)) end,
    is_cancelled = function() return host.progress_cancelled() and true or false end,
  }
  local out = function(level, ...)
    if type(level) ~= "number" then return A.debug.out(3, level, ...) end
    host.log(level, fmt(...))
  end
  A.debug = { out = out }
  A.log = function(...) local l = ... if type(l) == "number" then return out(...) end out(2, ...) end
  A.dialog = {
    display = function(dlg, buttons, ids) return host.dialog(dlg, buttons, ids) end,
    open = function(...) return host.file_dialog("open", ...) end,
    save = function(...) return host.file_dialog("save", ...) end,
  }
  A.text_extents = function(style, text) return host.text_extents(style, text or "") end
  A.parse_karaoke_data = parse_karaoke_data
  A.frame_from_ms = function(ms) return host.frame_from_ms(ms) end
  A.ms_from_frame = function(f) return host.ms_from_frame(f) end
  A.video_size = function() return host.video_size() end
  A.keyframes = function() return host.keyframes() end
  A.get_audio_selection = function() return host.audio_selection() end
  A.project_properties = function() return host.project_properties() end
  A.decode_path = function(p) return host.decode_path(p) end
  A.file_name = function() return host.file_name() end
  A.set_undo_point = function(desc) host.undo_point(desc or "") end
  A.clipboard = { get = function() return host.clipboard_get() end, set = function(s) host.clipboard_set(s) return true end }
  A.unicode = unicode
  A.util = util
  A.re = re
  A.cancel_marker = true
  return A
end

local function new_script(path)
  local script = { path = path, dir = path:match("^(.*)[/\\][^/\\]*$") or ".", macros = {}, filters = {}, loaded = {} }
  local env = setmetatable({}, { __index = _G })
  env._G = env
  env.aegisub = make_aegisub(script)
  env.unicode = unicode
  env.re = re
  install_utils_globals(env)
  env.include = function(name)
    local p = find_file(name, script)
    if not p then
      local b = builtin_includes[name]
      if b then return b(env) end
      error("Lua include not found: " .. name, 2)
    end
    if script.loaded[p] then return end
    script.loaded[p] = true
    local chunk = load_into(p, env)
    return chunk()
  end
  env.require = function(name)
    local b = builtin_includes[name]
    if b == nil and package.preload[name] == nil and package.loaded[name] == nil then
      local file = name:gsub("%.", "/") .. ".lua"
      local p = find_file(file, script) or find_file(name .. "/init.lua", script) or find_file(name:gsub("%.", "/") .. "/init.lua", script)
      if p and not package.loaded[p] then
        local chunk = load_into(p, env)
        local r = chunk(name)
        package.loaded[p] = (r == nil) and true or r
        return package.loaded[p]
      elseif p then return package.loaded[p] end
    end
    if b then return b(env) end
    return require(name)
  end
  script.env = env
  return script
end

-- module-style includes
package.preload["unicode"] = function() return unicode end
package.preload["aegisub.unicode"] = function() return unicode end
package.preload["re"] = function() return re end
package.preload["aegisub.re"] = function() return re end
package.preload["aegisub.util"] = function() return util end
package.preload["aegisub.argcheck"] = function() return function() end end
package.preload["clipboard"] = function() return { get = function() return host.clipboard_get() end, set = function(s) host.clipboard_set(s) return true end } end
package.preload["aegisub.clipboard"] = package.preload["clipboard"]

local O = {}

function O.set_include_dirs(dirs)
  include_dirs = {}
  for i = 1, #dirs do include_dirs[i] = dirs[i] end
end

function O.load_script(path)
  local script = new_script(path)
  scripts[#scripts + 1] = script
  local ok, err = pcall(function()
    local chunk = load_into(path, script.env)
    chunk()
  end)
  if not ok then
    if type(err) == "table" then err = err.message or "error" end
    script.error = tostring(err)
  end
  local e = script.env
  script.name = e.script_name or path:match("([^/\\]+)$")
  script.description = e.script_description or ""
  script.author = e.script_author or ""
  script.version = e.script_version or ""
  script.namespace = e.script_namespace or ""
  return script
end

-- classic karaskel-style entry points of old scripts (version 3 'process_lines' etc.) are not supported.

local function run_guarded(fn, ...)
  local res = { xpcall(fn, function(e)
    if type(e) == "table" and e.__cancel then return e end
    return { message = tostring(e), trace = debug.traceback(tostring(e), 2) }
  end, ...) }
  return res
end

local function selection_array(sel)
  local t = {}
  for i = 1, #sel do t[i] = sel[i] end
  return t
end

function O.make_subs(lines) return make_subs(lines) end

-- returns ok, selection(table|nil), active(number|nil), err(table|nil)
function O.run_macro(m, lines, selection, active)
  local subs = make_subs(lines)
  local sel = selection_array(selection)
  local res = run_guarded(m.process, subs, sel, active)
  if not res[1] then
    local e = res[2]
    if e.__cancel then return false, nil, nil, "cancelled" end
    return false, nil, nil, e.trace or e.message
  end
  local newsel, newactive = res[2], res[3]
  if type(newsel) == "table" then
    -- Aegisub accepts a table of indices; may return (sel, active) or only active
    local t = {}
    for i = 1, #newsel do t[i] = newsel[i] end
    newsel = t
  elseif type(newsel) == "number" then
    newactive = newsel
    newsel = nil
  else
    newsel = nil
  end
  return true, newsel, newactive
end

function O.validate_macro(m, lines, selection, active)
  if not m.validate then return true, "" end
  local subs = make_subs(lines)
  local res = run_guarded(m.validate, subs, selection_array(selection), active)
  if not res[1] then return false, "" end
  return res[2] and true or false, res[3] or ""
end

function O.run_filter(f, lines, config)
  local subs = make_subs(lines)
  local res = run_guarded(f.process, subs, config or {})
  if not res[1] then
    local e = res[2]
    if e.__cancel then return false, "cancelled" end
    return false, e.trace or e.message
  end
  return true
end

function O.filter_config(f, lines, old)
  if not f.config then return nil end
  local subs = make_subs(lines)
  local res = run_guarded(f.config, subs, old or {})
  if not res[1] then return nil end
  return res[2]
end

function O.new_lines() return {} end
function O.macros(script) local t = {} for i, m in ipairs(script.macros) do t[i] = m end return t end

O.parse_karaoke_data = parse_karaoke_data
O.builtin_includes = builtin_includes
O.util = util
O.unicode = unicode
return O
