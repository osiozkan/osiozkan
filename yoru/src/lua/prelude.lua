-- Yoru Lua çalışma ortamı: Aegisub 3.x otomasyon API'si (LuaJIT/5.1 uyumlu davranış, Lua 5.3 üzerinde)
local H = __host

---------------------------------------------------------------- Lua 5.1 uyumluluğu
unpack = unpack or table.unpack
loadstring = loadstring or function(s, n) return load(s, n) end
math.pow = math.pow or function(a, b) return a ^ b end
math.mod = math.mod or math.fmod
math.log10 = math.log10 or function(x) return math.log(x, 10) end
math.atan2 = math.atan2 or function(y, x) return math.atan(y, x) end
math.cosh = math.cosh or function(x) return (math.exp(x) + math.exp(-x)) / 2 end
math.sinh = math.sinh or function(x) return (math.exp(x) - math.exp(-x)) / 2 end
math.tanh = math.tanh or function(x) local a, b = math.exp(x), math.exp(-x); return (a - b) / (a + b) end
table.getn = table.getn or function(t) return #t end
table.maxn = table.maxn or function(t) local m = 0; for k in pairs(t) do if type(k) == "number" and k > m then m = k end end; return m end
string.gfind = string.gfind or string.gmatch
function setfenv(f, env)
  if type(f) == "number" then f = debug.getinfo(f + 1, "f").func end
  local i = 1
  while true do
    local name = debug.getupvalue(f, i)
    if name == "_ENV" then debug.upvaluejoin(f, i, function() return env end, 1); break
    elseif not name then break end
    i = i + 1
  end
  return f
end
function getfenv(f)
  if type(f) == "number" then f = debug.getinfo(f + 1, "f").func end
  local i = 1
  while true do
    local name, val = debug.getupvalue(f, i)
    if name == "_ENV" then return val elseif not name then break end
    i = i + 1
  end
  return _G
end
-- LuaJIT, tam sayı bekleyen yerlerde ondalıkları sessizce keser; Lua 5.3 hata verir. Aynı davranışı sağla.
do
  local function trunc(x)
    if math.type(x) == "float" then
      if x ~= x or x == math.huge or x == -math.huge then return x end
      return math.tointeger(x >= 0 and math.floor(x) or math.ceil(x)) or x
    end
    if type(x) == "string" then local n = tonumber(x); if n then return trunc(n) end end
    return x
  end
  local function wrap(tbl, name, ...)
    local f, idx = tbl[name], { ... }
    tbl[name] = function(...)
      local a = table.pack(...)
      for _, i in ipairs(idx) do if i <= a.n and a[i] ~= nil then a[i] = trunc(a[i]) end end
      return f(table.unpack(a, 1, a.n))
    end
  end
  wrap(string, "sub", 2, 3); wrap(string, "rep", 2); wrap(string, "byte", 2, 3); wrap(string, "char", 1, 2, 3, 4, 5, 6, 7, 8)
  wrap(table, "insert", 2); wrap(table, "remove", 2); wrap(table, "concat", 3, 4)
  local rnd = math.random
  math.random = function(m, n) if m ~= nil then m = trunc(m) end; if n ~= nil then n = trunc(n) end; return rnd(m, n) end
  local fmt = string.format
  string.format = function(f, ...)
    local hasfloat = false
    for i = 1, select("#", ...) do if math.type((select(i, ...))) == "float" then hasfloat = true; break end end
    if not hasfloat then return fmt(f, ...) end
    local args = table.pack(...)
    local k = 0
    for spec in f:gmatch("%%[%-%+ #0]*%d*%.?%d*([%a%%])") do
      if spec ~= "%" then
        k = k + 1
        if spec:find("[dicouxX]") and args[k] ~= nil then args[k] = trunc(args[k]) end
      end
    end
    return fmt(f, table.unpack(args, 1, args.n))
  end
end
-- 5.3'te tam sayı bölmesi/bit işleci farkları için LuaJIT 'bit' kütüphanesi
do
  local function tobit(x) x = math.tointeger(math.floor(x)) or 0; x = x & 0xFFFFFFFF; if x >= 0x80000000 then x = x - 0x100000000 end; return x end
  package.preload["bit"] = function()
    return {
      tobit = tobit,
      band = function(a, b, ...) local r = tobit(a) & tobit(b); for _, v in ipairs({...}) do r = r & tobit(v) end; return tobit(r) end,
      bor = function(a, b, ...) local r = tobit(a) | tobit(b); for _, v in ipairs({...}) do r = r | tobit(v) end; return tobit(r) end,
      bxor = function(a, b, ...) local r = tobit(a) ~ tobit(b); for _, v in ipairs({...}) do r = r ~ tobit(v) end; return tobit(r) end,
      bnot = function(a) return tobit(~tobit(a)) end,
      lshift = function(a, n) return tobit(tobit(a) << (n & 31)) end,
      rshift = function(a, n) return tobit((tobit(a) & 0xFFFFFFFF) >> (n & 31)) end,
      arshift = function(a, n) a = tobit(a); n = n & 31; local r = (a & 0xFFFFFFFF) >> n; if a < 0 and n > 0 then r = r | (0xFFFFFFFF << (32 - n)) end; return tobit(r) end,
      tohex = function(a, n) n = n or 8; return string.format("%0" .. math.abs(n) .. "x", tobit(a) & 0xFFFFFFFF):sub(-math.abs(n)) end,
    }
  end
end

---------------------------------------------------------------- JSON
local json = {}
do
  local esc = { ['"'] = '\\"', ['\\'] = '\\\\', ['\b'] = '\\b', ['\f'] = '\\f', ['\n'] = '\\n', ['\r'] = '\\r', ['\t'] = '\\t' }
  local function encstr(s)
    return '"' .. s:gsub('[%c"\\]', function(c) return esc[c] or string.format("\\u%04x", c:byte()) end) .. '"'
  end
  local function isarray(t)
    local n = 0
    for k in pairs(t) do
      if type(k) ~= "number" or k < 1 or k % 1 ~= 0 then return false end
      if k > n then n = k end
    end
    return n == #t
  end
  local function enc(v, out)
    local ty = type(v)
    if ty == "nil" then out[#out + 1] = "null"
    elseif ty == "boolean" then out[#out + 1] = tostring(v)
    elseif ty == "number" then
      if v ~= v or v == math.huge or v == -math.huge then out[#out + 1] = "null"
      elseif v % 1 == 0 and math.abs(v) < 1e15 then out[#out + 1] = string.format("%d", v)
      else out[#out + 1] = string.format("%.14g", v) end
    elseif ty == "string" then out[#out + 1] = encstr(v)
    elseif ty == "table" then
      if next(v) ~= nil and isarray(v) then
        out[#out + 1] = "["
        for i = 1, #v do if i > 1 then out[#out + 1] = "," end; enc(v[i], out) end
        out[#out + 1] = "]"
      else
        out[#out + 1] = "{"
        local first = true
        for k, x in pairs(v) do
          if type(x) ~= "function" and type(x) ~= "userdata" and type(x) ~= "thread" then
            if not first then out[#out + 1] = "," end
            first = false
            out[#out + 1] = encstr(tostring(k)); out[#out + 1] = ":"; enc(x, out)
          end
        end
        out[#out + 1] = "}"
      end
    else out[#out + 1] = "null" end
  end
  function json.encode(v) local out = {}; enc(v, out); return table.concat(out) end
  function json.encode_array(v) if next(v) == nil then return "[]" end; return json.encode(v) end

  local unesc = { b = "\b", f = "\f", n = "\n", r = "\r", t = "\t", ['"'] = '"', ['\\'] = '\\', ['/'] = '/' }
  function json.decode(s)
    local pos = 1
    local find, sub, byte = string.find, string.sub, string.byte
    local function ws() pos = find(s, "[^ \t\r\n]", pos) or #s + 1 end
    local val
    local function str()
      pos = pos + 1
      local buf, start = {}, pos
      while true do
        local q = find(s, '["\\]', pos)
        if not q then error("json: unterminated string") end
        if byte(s, q) == 34 then
          buf[#buf + 1] = sub(s, start, q - 1); pos = q + 1; break
        end
        buf[#buf + 1] = sub(s, start, q - 1)
        local c = sub(s, q + 1, q + 1)
        if c == "u" then
          local cp = tonumber(sub(s, q + 2, q + 5), 16)
          pos = q + 6
          if cp >= 0xD800 and cp < 0xDC00 and sub(s, pos, pos + 1) == "\\u" then
            local lo = tonumber(sub(s, pos + 2, pos + 5), 16)
            cp = 0x10000 + (cp - 0xD800) * 0x400 + (lo - 0xDC00); pos = pos + 6
          end
          buf[#buf + 1] = utf8.char(cp)
        else buf[#buf + 1] = unesc[c]; pos = q + 2 end
        start = pos
      end
      return table.concat(buf)
    end
    function val()
      ws()
      local c = byte(s, pos)
      if c == 123 then -- {
        pos = pos + 1; local t = {}; ws()
        if byte(s, pos) == 125 then pos = pos + 1; return t end
        while true do
          ws(); local k = str(); ws(); pos = pos + 1 -- ':'
          t[k] = val(); ws()
          local d = byte(s, pos); pos = pos + 1
          if d == 125 then break end
        end
        return t
      elseif c == 91 then -- [
        pos = pos + 1; local t, n = {}, 0; ws()
        if byte(s, pos) == 93 then pos = pos + 1; return t end
        while true do
          n = n + 1; t[n] = val(); ws()
          local d = byte(s, pos); pos = pos + 1
          if d == 93 then break end
        end
        return t
      elseif c == 34 then return str()
      elseif sub(s, pos, pos + 3) == "true" then pos = pos + 4; return true
      elseif sub(s, pos, pos + 4) == "false" then pos = pos + 5; return false
      elseif sub(s, pos, pos + 3) == "null" then pos = pos + 4; return nil
      else
        local a, b = find(s, "^-?%d+%.?%d*[eE]?[+-]?%d*", pos)
        local num = tonumber(sub(s, a, b)); pos = b + 1
        return num
      end
    end
    return val()
  end
end

---------------------------------------------------------------- Altyazı nesnesi
local raw_mt
local function copy_line(l)
  local c = {}
  for k, v in pairs(l) do if k ~= "raw" then c[k] = v end end
  setmetatable(c, raw_mt)
  if l.extra then c.extra = {}; for k, v in pairs(l.extra) do c.extra[k] = v end end
  return c
end

local make_raw
local function fmt_time(ms)
  ms = math.max(0, math.floor(ms + 0.5))
  local cs = (ms // 10) % 100
  local t = ms // 1000
  return string.format("%d:%02d:%02d.%02d", t // 3600, (t // 60) % 60, t % 60, cs)
end
local function color_str(c) return c end

function make_raw(l)
  if l.class == "dialogue" then
    return string.format("%s: %d,%s,%s,%s,%s,%d,%d,%d,%s,%s", l.comment and "Comment" or "Dialogue", l.layer or 0,
      fmt_time(l.start_time or 0), fmt_time(l.end_time or 0), l.style or "Default", l.actor or "",
      l.margin_l or 0, l.margin_r or 0, l.margin_t or 0, l.effect or "", l.text or "")
  elseif l.class == "style" then
    local function b(v) return v and -1 or 0 end
    return string.format("Style: %s,%s,%s,%s,%s,%s,%s,%d,%d,%d,%d,%s,%s,%s,%s,%d,%s,%s,%d,%d,%d,%d,%d",
      l.name, l.fontname, l.fontsize, l.color1, l.color2, l.color3, l.color4, b(l.bold), b(l.italic), b(l.underline), b(l.strikeout),
      l.scale_x, l.scale_y, l.spacing, l.angle, l.borderstyle, l.outline, l.shadow, l.align, l.margin_l, l.margin_r, l.margin_t, l.encoding)
  elseif l.class == "info" then return tostring(l.key) .. ": " .. tostring(l.value) end
  return l.raw or ""
end

raw_mt = { __index = function(t, k) if k == "raw" then return make_raw(t) end end }
local function norm_color(c) c = tostring(c or "&H00FFFFFF"); if c:sub(-1) ~= "&" then c = c .. "&" end; return c end
local function from_js_style(s)
  return { class = "style", section = "[V4+ Styles]", name = s.name, fontname = s.fontname, fontsize = s.fontsize,
    color1 = norm_color(s.primary), color2 = norm_color(s.secondary), color3 = norm_color(s.outline), color4 = norm_color(s.back),
    bold = s.bold, italic = s.italic, underline = s.underline, strikeout = s.strikeout,
    scale_x = s.scalex, scale_y = s.scaley, spacing = s.spacing, angle = s.angle, borderstyle = s.borderStyle,
    outline = s.outlineW, shadow = s.shadow, align = s.alignment, margin_l = s.marginL, margin_r = s.marginR,
    margin_t = s.marginV, margin_b = s.marginV, margin_v = s.marginV, encoding = s.encoding }
end
local function to_js_style(l)
  local function c(x) x = tostring(x); if x:sub(-1) == "&" then x = x:sub(1, -2) end; return x end
  return { name = l.name, fontname = l.fontname, fontsize = l.fontsize, primary = c(l.color1), secondary = c(l.color2),
    outline = c(l.color3), back = c(l.color4), bold = l.bold and true or false, italic = l.italic and true or false,
    underline = l.underline and true or false, strikeout = l.strikeout and true or false,
    scalex = l.scale_x, scaley = l.scale_y, spacing = l.spacing, angle = l.angle, borderStyle = l.borderstyle,
    outlineW = l.outline, shadow = l.shadow, alignment = l.align, marginL = l.margin_l, marginR = l.margin_r,
    marginV = l.margin_t or l.margin_v or 0, encoding = l.encoding }
end

local function build_lines(data)
  local lines = {}
  for _, kv in ipairs(data.info) do
    local l = { class = "info", section = "[Script Info]", key = kv[1], value = kv[2] }
    setmetatable(l, raw_mt); lines[#lines + 1] = l
  end
  for _, s in ipairs(data.styles) do local l = from_js_style(s); setmetatable(l, raw_mt); lines[#lines + 1] = l end
  for _, e in ipairs(data.events) do
    local l = { class = "dialogue", section = "[Events]", comment = e.comment, layer = e.layer, start_time = e.start, end_time = e["end"],
      style = e.style, actor = e.actor, margin_l = e.marginL, margin_r = e.marginR, margin_t = e.marginV, margin_b = e.marginV,
      margin_v = e.marginV, effect = e.effect, text = e.text, extra = {} }
    setmetatable(l, raw_mt); lines[#lines + 1] = l
  end
  return lines
end

local function dump_lines(lines)
  local out = { info = {}, styles = {}, events = {} }
  for _, l in ipairs(lines) do
    if l.class == "info" then out.info[#out.info + 1] = { l.key, tostring(l.value) }
    elseif l.class == "style" then out.styles[#out.styles + 1] = to_js_style(l)
    elseif l.class == "dialogue" then
      local mt = l.margin_t or l.margin_v or 0
      out.events[#out.events + 1] = { comment = l.comment and true or false, layer = l.layer or 0, start = math.floor((l.start_time or 0) + 0.5),
        ["end"] = math.floor((l.end_time or 0) + 0.5), style = l.style or "Default", actor = l.actor or "", marginL = l.margin_l or 0,
        marginR = l.margin_r or 0, marginV = mt, effect = l.effect or "", text = l.text or "" }
    end
  end
  return out
end

local function new_subs(lines)
  local subs = {}
  local methods = {}
  function methods.insert(idx, ...)
    local items = { ... }
    for i, l in ipairs(items) do table.insert(lines, idx + i - 1, copy_line(l)) end
  end
  function methods.append(...) for _, l in ipairs({ ... }) do lines[#lines + 1] = copy_line(l) end end
  function methods.delete(...)
    local idx = { ... }
    if type(idx[1]) == "table" then idx = idx[1] end
    table.sort(idx, function(a, b) return a > b end)
    for _, i in ipairs(idx) do table.remove(lines, i) end
  end
  function methods.deleterange(a, b) for i = b, a, -1 do table.remove(lines, i) end end
  function methods.n() return #lines end
  methods.insert_unchecked = methods.insert
  return setmetatable(subs, {
    __index = function(_, k)
      if k == "n" then return #lines end
      if type(k) == "number" then
        local l = lines[k]
        if not l then error("Requested out-of-range line from subtitle file: " .. tostring(k), 2) end
        return copy_line(l)
      end
      return methods[k]
    end,
    __newindex = function(_, k, v)
      if type(k) ~= "number" or k < 1 or k > #lines + 1 then error("Attempt to write out-of-range line to subtitle file: " .. tostring(k), 2) end
      v = copy_line(v); lines[k] = v
    end,
    __len = function() return #lines end,
  })
end

---------------------------------------------------------------- aegisub API
local cancel_marker = {}
aegisub = {
  lua_supported_version = 4, lua_api_version = "3.0", yoru = true,
  cancel = function() error(cancel_marker, 0) end,
  gettext = function(s) return s end,
  set_undo_point = function() end,
  progress = { set = function(v) H.progress("set", tostring(v)) end, task = function(t) H.progress("task", tostring(t)) end, title = function(t) H.progress("title", tostring(t)) end,
    is_cancelled = function() return false end },
  debug = {}, dialog = {}, clipboard = {},
}
function aegisub.log(level, msg, ...)
  if type(level) ~= "number" then msg, level = level, 1; return aegisub.log(1, msg, ...) end
  local ok, s = pcall(string.format, msg, ...)
  H.log(level, ok and s or tostring(msg))
end
aegisub.debug.out = aegisub.log
aegisub.debug.get_level = function() return 3 end
function aegisub.text_extents(style, text)
  local r = H.extents(json.encode({ fontname = style.fontname, fontsize = style.fontsize, bold = style.bold, italic = style.italic,
    scale_x = style.scale_x, scale_y = style.scale_y, spacing = style.spacing }), text)
  local w, h, d, e = r:match("([%-%d%.]+)|([%-%d%.]+)|([%-%d%.]+)|([%-%d%.]+)")
  return tonumber(w), tonumber(h), tonumber(d), tonumber(e)
end
function aegisub.frame_from_ms(ms) local r = H.frame_from_ms(ms); return r ~= -1 and r or nil end
function aegisub.ms_from_frame(f) local r = H.ms_from_frame(f); return r ~= -1 and r or nil end
function aegisub.video_size()
  local w, h = H.video_size():match("(%d+)|(%d+)")
  if not w or w == "0" then return nil end
  return tonumber(w), tonumber(h), tonumber(w) / tonumber(h), 0
end
function aegisub.keyframes() return json.decode(H.keyframes()) or {} end
function aegisub.decode_path(p) return (tostring(p):gsub("^%?data", "."):gsub("^%?user", "."):gsub("^%?script", ".")) end
function aegisub.project_properties() return json.decode(H.project()) or {} end
function aegisub.file_name() return H.project_name() end
function aegisub.get_audio_selection() return nil end
function aegisub.get_cursor() return nil end
function aegisub.clipboard.get() return H.clipboard_get() end
function aegisub.clipboard.set(s) H.clipboard_set(tostring(s)) return true end
aegisub.get_clipboard = aegisub.clipboard.get
aegisub.set_clipboard = aegisub.clipboard.set
function aegisub.dialog.display(config, buttons, ids)
  buttons = buttons or { "OK", "Cancel" }
  local resp = coroutine.yield({ __req = "dialog", config = config, buttons = buttons, ids = ids })
  if not resp or resp.button == nil or resp.button == false then return false, {} end
  return resp.button, resp.values or {}
end
function aegisub.dialog.open(title, default_name, dir, wildcards, multiple, must_exist)
  local resp = coroutine.yield({ __req = "open", title = title, wildcards = wildcards, multiple = multiple })
  return resp and resp.path or nil
end
function aegisub.dialog.save(title, default_name, dir, wildcards, overwrite_prompt)
  local resp = coroutine.yield({ __req = "save", title = title, default = default_name, wildcards = wildcards })
  return resp and resp.path or nil
end

---------------------------------------------------------------- unicode
unicode = {}
function unicode.len(s) return utf8.len(s) or #s end
function unicode.codepoint(c) return utf8.codepoint(c, 1) end
function unicode.chars(s)
  local iter = s:gmatch(utf8.charpattern)
  return function() return iter() end
end
function unicode.charwidth(c) return 1 end
function unicode.to_upper(s) return H.upper(s) end
function unicode.to_lower(s) return H.lower(s) end
unicode.to_fold = unicode.to_lower
package.preload["unicode"] = function() return unicode end
package.preload["aegisub.unicode"] = function() return unicode end

---------------------------------------------------------------- re (Boost.Regex yerine JS RegExp)
re = {}
local function re_call(op, ...) return json.decode(H.re(json.encode({ op = op, args = { ... } }))) end
local function wrap_match(m)
  if not m then return nil end
  local out = {}
  for i, g in ipairs(m) do out[i] = { str = g.str, first = g.first, last = g.last } end
  return out
end
local re_flags = { ICASE = 1, NOSUB = 2, NO_MOD_M = 4, NO_MOD_S = 8, NO_MOD_X = 16, NO_EMPTY_SUBEXPRESSIONS = 32, SYNTAX_PERL = 64 }
for k, v in pairs(re_flags) do re[k] = v end
function re.find(str, pat, flags) local r = re_call("find", str, pat, flags or 0)
  if not r or #r == 0 then return nil end
  local res = {}; for i, m in ipairs(r) do res[i] = wrap_match(m)[1] end; return res end
function re.match(str, pat, flags)
  local r = re_call("match", str, pat, flags or 0)
  if not r then return nil end
  return wrap_match(r)
end
function re.sub(str, pat, repl, count, flags) return re_call("sub", str, pat, repl, count or 0, flags or 0) end
function re.gsub(str, pat, repl, flags) return re_call("sub", str, pat, repl, 0, flags or 0) end
function re.split(str, pat, flags) return re_call("split", str, pat, flags or 0) end
function re.compile(pat, flags)
  local o = {}
  function o:find(s) return re.find(s, pat, flags) end
  function o:match(s) return re.match(s, pat, flags) end
  function o:sub(s, r, n) return re.sub(s, pat, r, n, flags) end
  function o:gsub(s, r) return re.sub(s, pat, r, 0, flags) end
  return o
end
package.preload["re"] = function() return re end
package.preload["aegisub.re"] = function() return re end

---------------------------------------------------------------- script yükleme / include
local script_env_mt = { __index = _G }
local scripts = {}
local macros = {}
local current

local function embedded(name)
  name = name:gsub("%.lua$", "")
  return H.embedded(name)
end

local function make_env()
  local env = setmetatable({ aegisub = aegisub, unicode = unicode, re = re }, script_env_mt)
  env._G = env
  env.include = function(file)
    local src = embedded(file)
    if src then
      local f, err = load(src, "=" .. file, "t", env)
      if not f then error(err, 2) end
      f(); return
    end
    src = H.read_script_file(file)
    if not src then error("Lua include not found: " .. tostring(file), 2) end
    local f, err = load(src, "=" .. file, "t", env)
    if not f then error(err, 2) end
    f()
  end
  env.require = function(mod)
    local pl = package.loaded[mod]
    if pl then return pl end
    local src = embedded(mod)
    if src and not package.preload[mod] then
      local f, err = load(src, "=" .. mod, "t", env)
      if not f then error(err, 2) end
      local r = f(); package.loaded[mod] = r == nil and true or r; return package.loaded[mod]
    end
    return require(mod)
  end
  env.dofile = function(file) return env.include(file) end
  return env
end

function __load_script(name, src)
  local env = make_env()
  local f, err = load(src, "=" .. name, "t", env)
  if not f then return json.encode({ ok = false, message = tostring(err) }) end
  local meta = { name = name }
  local before = #macros
  current = { env = env, macros = {} }
  -- register_* bu betik ortamını kullanır
  local ok, e = xpcall(f, function(m) return tostring(m) .. "\n" .. debug.traceback("", 2) end)
  if not ok then return json.encode({ ok = false, message = tostring(e) }) end
  meta.script_name = env.script_name or name
  meta.script_description = env.script_description or ""
  meta.script_author = env.script_author or ""
  meta.script_version = env.script_version or ""
  -- eski tarz: yalnızca process_lines? (Aegisub 3'te yok) — macro yoksa uyar
  local list = {}
  for _, m in ipairs(current.macros) do list[#list + 1] = { id = m.id, name = m.name, desc = m.desc, kind = m.kind } end
  scripts[name] = meta
  return json.encode({ ok = true, meta = meta, macros = list })
end

local next_id = 0
local function register(kind, name, desc, fn, validate, is_active, extra)
  next_id = next_id + 1
  local m = { id = next_id, kind = kind, name = name, desc = desc, fn = fn, validate = validate, is_active = is_active, extra = extra }
  macros[next_id] = m
  current.macros[#current.macros + 1] = m
end
function aegisub.register_macro(name, desc, fn, validate, is_active) register("macro", name, desc, fn, validate, is_active) end
function aegisub.register_filter(name, desc, prio, fn, config) register("filter", name, desc, fn, nil, nil, { prio = prio, config = config }) end

local function rebuild_env_hooks()
  -- scripts register via aegisub.* which is shared; ok
end

---------------------------------------------------------------- yürütme
local run = {}
local function make_input(data)
  local lines = build_lines(data)
  return lines, new_subs(lines)
end

function __validate(id, data)
  local m = macros[id]
  if not m then return { ok = false } end
  if not m.validate then return { ok = true } end
  local lines, subs = make_input(data)
  local ok, r, msg = pcall(m.validate, subs, data.sel or {}, data.active or 1)
  if not ok then return { ok = false, message = tostring(r) } end
  return { ok = r and true or false, message = msg }
end

local function finish(ra, rb)
  local sel, active
  if type(ra) == "table" then sel = ra; active = rb elseif type(ra) == "number" then sel = { ra }; active = rb end
  local out = dump_lines(run.lines)
  out.status = "done"; out.sel = sel; out.active = active
  return out
end

local function step(resp)
  local res = table.pack(coroutine.resume(run.co, resp))
  local ok = res[1]
  if not ok then
    local e = res[2]
    if e == cancel_marker then return { status = "cancelled" } end
    return { status = "error", message = tostring(e) }
  end
  if coroutine.status(run.co) == "suspended" then
    return { status = "request", req = res[2] }
  end
  -- bitti: xpcall(fn) → true, r1, r2
  if res[2] == false then
    local e = res[3]
    if e == cancel_marker then return { status = "cancelled" } end
    return { status = "error", message = tostring(e) }
  end
  return finish(res[3], res[4])
end

function __start(id, data)
  local m = macros[id]
  if not m then return { status = "error", message = "unknown macro" } end
  local lines, subs = make_input(data)
  run = { lines = lines, subs = subs }
  local sel = data.sel or {}
  local active = data.active or sel[1] or 1
  if m.kind == "filter" then
    run.co = coroutine.create(function()
      return xpcall(function()
        local cfg = {}
        if m.extra and m.extra.config then
          local c = type(m.extra.config) == "function" and m.extra.config(subs, {}) or m.extra.config
          if type(c) == "table" and #c > 0 then
            local b, v = aegisub.dialog.display(c, { "OK", "Cancel" })
            if not b then aegisub.cancel() end
            cfg = v
          end
        end
        return m.fn(subs, cfg)
      end, function(e) if e == cancel_marker then return e end; return tostring(e) .. "\n" .. debug.traceback("", 2) end)
    end)
  else
    run.co = coroutine.create(function()
      return xpcall(m.fn, function(e) if e == cancel_marker then return e end; return tostring(e) .. "\n" .. debug.traceback("", 2) end, subs, sel, active)
    end)
  end
  return step(nil)
end

function __resume(resp) return step(resp) end

package.preload["aegisub.json"] = function() return json end
__json = json
