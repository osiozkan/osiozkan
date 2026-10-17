-- OsiSub Automation 4 başlangıç kodu.
-- Aegisub'ın C++ tarafında sunduğu `aegisub` tablosunu ve subtitles
-- nesnesini Lua içinde kurar; ana makineye (Python) yalnız `py` tablosu
-- üzerinden çıkar. Bu dosya her betiğin kendi Lua durumunda bir kez çalışır.

local py = ...            -- Python geri çağırımları
local osi = {}            -- Python'a döndürülen iç API
rawset(_G, "__osisub_py", py)   -- include/aegisub/*.lua modülleri buradan erişir

---------------------------------------------------------------------------
-- LuaJIT 5.1 uyumluluk ekleri (Aegisub LuaJIT'i 5.2 uyumuyla derler)
---------------------------------------------------------------------------
table.unpack = table.unpack or unpack
table.pack = table.pack or function(...) return {n = select('#', ...), ...} end
do
  -- 5.2 uyumu: __ipairs / __pairs üst yöntemleri (karaskel `ipairs(subs)` kullanır)
  local raw_ipairs, raw_pairs = ipairs, pairs
  function ipairs(t)
    local mt = getmetatable(t)
    if type(mt) == "table" and mt.__ipairs then return mt.__ipairs(t) end
    return raw_ipairs(t)
  end
  function pairs(t)
    local mt = getmetatable(t)
    if type(mt) == "table" and mt.__pairs then return mt.__pairs(t) end
    return raw_pairs(t)
  end
  local log = math.log
  math.log = function(x, base)
    if base then return log(x) / log(base) end
    return log(x)
  end
  local rep = string.rep
  string.rep = function(s, n, sep)
    if sep and n > 1 then return rep(s .. sep, n - 1) .. s end
    return rep(s, n)
  end
end

---------------------------------------------------------------------------
-- İptal işareti ve yardımcılar
---------------------------------------------------------------------------
local CANCEL = setmetatable({}, {__tostring = function() return "aegisub.cancel()" end})
osi.CANCEL = CANCEL

local function shallow_copy(t)
  local c = {}
  for k, v in pairs(t) do
    if k == "extra" and type(v) == "table" then
      local e = {}
      for k2, v2 in pairs(v) do e[k2] = v2 end
      c[k] = e
    else
      c[k] = v
    end
  end
  return c
end
osi.copy = shallow_copy

local function fmt_time(ms)
  ms = math.floor((tonumber(ms) or 0) + 5)
  ms = ms - ms % 10
  if ms < 0 then ms = 0 end
  local cs = math.floor(ms / 10)
  local h = math.floor(cs / 360000); cs = cs % 360000
  local m = math.floor(cs / 6000); cs = cs % 6000
  local s = math.floor(cs / 100); cs = cs % 100
  return string.format("%d:%02d:%02d.%02d", h, m, s, cs)
end

local function fmt_num(v)
  v = tonumber(v) or 0
  if v == math.floor(v) then return string.format("%d", v) end
  local s = string.format("%.3f", v):gsub("0+$", ""):gsub("%.$", "")
  return s
end

local function b2s(b) return b and "-1" or "0" end

local function line_raw(l)
  if l.class == "dialogue" then
    return (l.comment and "Comment: " or "Dialogue: ") .. table.concat({
      tostring(l.layer or 0), fmt_time(l.start_time), fmt_time(l.end_time),
      tostring(l.style or ""), tostring(l.actor or ""),
      tostring(l.margin_l or 0), tostring(l.margin_r or 0), tostring(l.margin_t or 0),
      tostring(l.effect or ""), tostring(l.text or "")}, ",")
  elseif l.class == "style" then
    return "Style: " .. table.concat({
      tostring(l.name), tostring(l.fontname), fmt_num(l.fontsize),
      tostring(l.color1), tostring(l.color2), tostring(l.color3), tostring(l.color4),
      b2s(l.bold), b2s(l.italic), b2s(l.underline), b2s(l.strikeout),
      fmt_num(l.scale_x), fmt_num(l.scale_y), fmt_num(l.spacing), fmt_num(l.angle),
      tostring(l.borderstyle), fmt_num(l.outline), fmt_num(l.shadow), tostring(l.align),
      tostring(l.margin_l), tostring(l.margin_r), tostring(l.margin_t), tostring(l.encoding)}, ",")
  elseif l.class == "info" then
    return tostring(l.key) .. ": " .. tostring(l.value)
  end
  return l.raw or ""
end
osi.line_raw = line_raw

---------------------------------------------------------------------------
-- subtitles nesnesi
---------------------------------------------------------------------------
-- lines : Python'un kurduğu satır tabloları dizisi (1'den başlar)
-- meta  : [depolanan tablo] = {pristine=pyidx} | {src=pyidx, slot=pyidx}
-- Betiğe her zaman KOPYA verilir; atamada da kopya saklanır (Aegisub da
-- satırı C++ nesnesine çevirdiği için betik tablosunu sonradan değiştirmek
-- belgeyi etkilemez).
function osi.make_subs(lines, readonly)
  local meta = {}
  for i = 1, #lines do meta[lines[i]] = {pristine = i} end
  local srcmap = setmetatable({}, {__mode = "k"})
  local state = {lines = lines, meta = meta, modified = false}

  local function origin_of(t)
    local m = meta[t]
    if not m then return nil end
    return m.pristine or m.src
  end

  local function check_index(i, max)
    if type(i) ~= "number" then error("Satır indeksi sayı olmalı, " .. type(i) .. " verildi", 3) end
    if i ~= math.floor(i) or i < 1 or i > max then
      error(string.format("Satır indeksi aralık dışında: %s (1..%d)", tostring(i), max), 3)
    end
  end

  local function check_line(l)
    if type(l) ~= "table" then error("Satır tablo olmalı", 3) end
    if type(l.class) ~= "string" then error("Satırın 'class' alanı yok", 3) end
  end

  local function check_rw()
    if readonly then error("Doğrulama işlevinde altyazı değiştirilemez", 3) end
    state.modified = true
  end

  local function store_new(l)
    check_line(l)
    local c = shallow_copy(l)
    meta[c] = {src = srcmap[l]}
    return c
  end

  local function get(i)
    check_index(i, #lines)
    local t = lines[i]
    local c = shallow_copy(t)
    if not (meta[t] and meta[t].pristine) then c.raw = line_raw(c) end
    srcmap[c] = origin_of(t)
    return c
  end

  local function flatten(out, ...)
    for i = 1, select('#', ...) do
      local v = select(i, ...)
      if type(v) == "table" and v.class == nil and #v > 0 then
        for _, x in ipairs(v) do out[#out + 1] = x end
      else
        out[#out + 1] = v
      end
    end
    return out
  end

  local methods = {}
  -- Aegisub AssFile::GetResolution ile aynı varsayımlar
  function methods.script_resolution()
    local x, y = 0, 0
    for _, l in ipairs(lines) do
      if l.class == "info" then
        local k = tostring(l.key):lower()
        if k == "playresx" then x = math.floor(tonumber(l.value) or 0) end
        if k == "playresy" then y = math.floor(tonumber(l.value) or 0) end
      elseif l.class == "dialogue" then
        break
      end
    end
    if x <= 0 and y <= 0 then return 384, 288 end
    if x <= 0 then x = (y == 1024) and 1280 or math.floor(y * 4 / 3) end
    if y <= 0 then y = (x == 1280) and 1024 or math.floor(x * 3 / 4) end
    return x, y
  end
  function methods.insert(i, ...)
    check_rw()
    check_index(i, #lines + 1)
    local new = flatten({}, ...)
    for k = #new, 1, -1 do
      table.insert(lines, i, store_new(new[k]))
    end
  end
  function methods.append(...)
    check_rw()
    local new = flatten({}, ...)
    for k = 1, #new do lines[#lines + 1] = store_new(new[k]) end
  end
  function methods.delete(...)
    check_rw()
    local idx = {}
    for i = 1, select('#', ...) do
      local v = select(i, ...)
      if type(v) == "table" then
        for _, x in ipairs(v) do idx[#idx + 1] = x end
      else
        idx[#idx + 1] = v
      end
    end
    table.sort(idx, function(a, b) return a > b end)
    local last
    for _, i in ipairs(idx) do
      if i ~= last then
        check_index(i, #lines)
        table.remove(lines, i)
        last = i
      end
    end
  end
  function methods.deleterange(a, b)
    check_rw()
    if b < a then return end
    check_index(a, #lines); check_index(b, #lines)
    for i = b, a, -1 do table.remove(lines, i) end
  end

  local proxy = newproxy(true)
  local mt = getmetatable(proxy)
  mt.__index = function(_, k)
    if type(k) == "number" then return get(k) end
    if k == "n" then return #lines end
    local m = methods[k]
    if m then return m end
    error("subtitles nesnesinde '" .. tostring(k) .. "' yok", 2)
  end
  mt.__newindex = function(_, k, v)
    check_rw()
    if type(k) ~= "number" then error("subtitles nesnesine yalnız sayı indeksiyle atama yapılır", 2) end
    check_line(v)
    local n = #lines
    if k > 0 and k <= n then
      local old = lines[k]
      local om = meta[old] or {}
      local slot = om.pristine or om.slot
      local c = shallow_copy(v)
      meta[c] = {src = srcmap[v] or slot, slot = slot}
      lines[k] = c
    elseif k == 0 or k == n + 1 then
      lines[n + 1] = store_new(v)
    elseif k < 0 and -k <= n + 1 then
      table.insert(lines, -k, store_new(v))
    else
      error(string.format("Satır indeksi aralık dışında: %d", k), 2)
    end
  end
  mt.__len = function() return #lines end
  mt.__ipairs = function()
    local i = 0
    return function()
      i = i + 1
      if i <= #lines then return i, get(i) end
    end, proxy, 0
  end
  mt.__tostring = function() return "subtitles (" .. #lines .. " satır)" end
  return proxy, state
end

---------------------------------------------------------------------------
-- Betik kayıtları
---------------------------------------------------------------------------
osi.macros = {}
osi.filters = {}

local aegisub = {}
_G.aegisub = aegisub
aegisub.lua_automation_version = 4

function aegisub.register_macro(name, description, process, validate, is_active)
  if type(name) ~= "string" then error("register_macro: ad metin olmalı", 2) end
  if type(process) ~= "function" then error("register_macro: işlem işlevi gerekli", 2) end
  osi.macros[#osi.macros + 1] = {name = name, description = description or "",
    process = process, validate = validate, is_active = is_active}
end

function aegisub.register_filter(name, description, priority, process, config)
  if type(process) ~= "function" then error("register_filter: işlem işlevi gerekli", 2) end
  osi.filters[#osi.filters + 1] = {name = name, description = description or "",
    priority = priority or 0, process = process, config = config}
end

function aegisub.cancel() error(CANCEL, 0) end
function aegisub.gettext(s) return s end
function aegisub.text_extents(style, text)
  if type(style) ~= "table" then error("text_extents: stil tablosu gerekli", 2) end
  return py.text_extents(style, tostring(text or ""))
end
function aegisub.frame_from_ms(ms) return py.frame_from_ms(tonumber(ms) or 0) end
function aegisub.ms_from_frame(f) return py.ms_from_frame(tonumber(f) or 0) end
function aegisub.video_size() return py.video_size() end
function aegisub.keyframes() return py.keyframes() end
function aegisub.get_audio_selection() return py.audio_selection() end
function aegisub.decode_path(p) return py.decode_path(tostring(p)) end
function aegisub.file_name() return py.file_name() end
function aegisub.project_properties() return py.project_properties() end
function aegisub.set_undo_point(desc) py.set_undo_point(tostring(desc or "")) end
-- arch1t3cht 3.4: kare nesnesi (:width, :height, :getPixel, :getPixelFormatted)
local FRAME_MT = {}
FRAME_MT.__index = FRAME_MT
function FRAME_MT:width() return self._w end
function FRAME_MT:height() return self._h end
function FRAME_MT:getPixel(x, y) return py.frame_pixel(self._id, tonumber(x) or 0, tonumber(y) or 0) end
function FRAME_MT:getPixelFormatted(x, y)
  local v = py.frame_pixel(self._id, tonumber(x) or 0, tonumber(y) or 0)
  if not v then return nil end
  local r, g, b = math.floor(v / 65536) % 256, math.floor(v / 256) % 256, v % 256
  return string.format("&H%02X%02X%02X&", b, g, r)
end
function aegisub.get_frame(frame, with_subs)
  local id, w, h = py.get_frame(tonumber(frame) or 0, with_subs and true or false)
  if not id then return nil end
  return setmetatable({_id = id, _w = w, _h = h}, FRAME_MT)
end

-- arch1t3cht 3.4 uzantıları (düzenleme kutusu imleci/seçimi)
aegisub.gui = {
  get_cursor = function() return py.gui_get_cursor() end,
  set_cursor = function(p) py.gui_set_cursor(tonumber(p) or 1) end,
  get_selection = function() return py.gui_get_selection() end,
  set_selection = function(a, b) py.gui_set_selection(tonumber(a) or 1, tonumber(b) or 1) end,
  is_modified = function() return py.gui_is_modified() end,
}

local function log_impl(level, fmt, ...)
  if type(level) ~= "number" then
    return log_impl(-1, level, fmt, ...)
  end
  local ok, msg
  if select('#', ...) > 0 then
    ok, msg = pcall(string.format, tostring(fmt), ...)
    if not ok then msg = tostring(fmt) end
  else
    msg = tostring(fmt)
  end
  py.log(level, msg)
end
aegisub.debug = {out = log_impl}
aegisub.log = log_impl

aegisub.progress = {
  set = function(p) py.progress_set(tonumber(p) or 0) end,
  task = function(fmt, ...)
    local ok, s = pcall(string.format, tostring(fmt or ""), ...)
    py.progress_task(ok and s or tostring(fmt))
  end,
  title = function(fmt, ...)
    local ok, s = pcall(string.format, tostring(fmt or ""), ...)
    py.progress_title(ok and s or tostring(fmt))
  end,
  is_cancelled = function() return py.is_cancelled() end,
}

aegisub.dialog = {
  display = function(dialog, buttons, button_ids)
    if type(dialog) ~= "table" then error("dialog.display: tablo gerekli", 2) end
    local pressed, results = py.dialog_display(dialog, buttons, button_ids)
    if pressed == nil then pressed = false end
    return pressed, results
  end,
  open = function(title, default_file, default_dir, wildcards, allow_multiple, must_exist)
    return py.dialog_open(title or "", default_file or "", default_dir or "",
      wildcards or "", allow_multiple and true or false, must_exist ~= false)
  end,
  save = function(title, default_file, default_dir, wildcards, dont_prompt_overwrite)
    return py.dialog_save(title or "", default_file or "", default_dir or "",
      wildcards or "", dont_prompt_overwrite and true or false)
  end,
}

---------------------------------------------------------------------------
-- include() ve require yolları
---------------------------------------------------------------------------
osi.script_dir = ""
osi.include_dirs = {}

-- Dosya erişimi Python üzerinden: Windows'ta Türkçe karakterli yollar
-- (Masaüstü, Belgeler...) LuaJIT'in fopen'ıyla açılamaz.
local function file_exists(p) return py.file_exists(p) end

local function read_chunk(path)
  local src = py.read_file(path)
  if not src then return nil, "açılamadı: " .. path end
  if src:sub(1, 3) == "\239\187\191" then src = src:sub(4) end   -- UTF-8 BOM
  return loadstring(src, "@" .. path)
end
osi.read_chunk = read_chunk

function include(name)
  if type(name) ~= "string" then error("include: dosya adı gerekli", 2) end
  local candidates = {}
  if name:match("^%a:[/\\]") or name:sub(1, 1) == "/" then
    candidates[1] = name
  else
    candidates[#candidates + 1] = osi.script_dir .. "/" .. name
    for _, d in ipairs(osi.include_dirs) do candidates[#candidates + 1] = d .. "/" .. name end
  end
  for _, p in ipairs(candidates) do
    if file_exists(p) then
      local chunk, err = read_chunk(p)
      if not chunk then error(err, 2) end
      return chunk()
    end
  end
  error("include: '" .. name .. "' bulunamadı", 2)
end

function osi.set_paths(script_dir, include_dirs)
  osi.script_dir = script_dir
  osi.include_dirs = include_dirs
  local parts = {}
  for _, d in ipairs(include_dirs) do
    parts[#parts + 1] = d .. "/?.lua"
    parts[#parts + 1] = d .. "/?/init.lua"
  end
  package.path = table.concat(parts, ";")
  package.cpath = ""
end

-- Aegisub'ın C++'a bağlı modülleri: kendi uygulamalarımız önce gelir.
function osi.preload(name, path)
  package.preload[name] = function(...)
    local chunk, err = read_chunk(path)
    if not chunk then error(err) end
    return chunk(...)
  end
end

---------------------------------------------------------------------------
-- Betik yükleme ve çalıştırma
---------------------------------------------------------------------------
local function traceback(e)
  if e == CANCEL then return CANCEL end
  if type(e) == "table" then
    local ok, s = pcall(tostring, e)
    e = ok and s or "(tablo hata)"
  end
  return debug.traceback(tostring(e), 2)
end

function osi.load_script(path)
  local chunk, err = read_chunk(path)
  if not chunk then return false, tostring(err) end
  local ok, e = xpcall(chunk, traceback)
  if not ok then return false, tostring(e) end
  return true, {
    name = rawget(_G, "script_name"),
    description = rawget(_G, "script_description"),
    author = rawget(_G, "script_author"),
    version = rawget(_G, "script_version"),
    namespace = rawget(_G, "script_namespace"),
  }
end

local function result_of(ok, ...)
  if ok then return "ok", ... end
  local e = ...
  if e == CANCEL then return "cancel" end
  return "error", tostring(e)
end

-- Sert iptal: betik aegisub.progress.is_cancelled()'a hiç bakmasa bile,
-- kullanıcı iptalde ısrar ederse yorumlanan kodda belirli aralıklarla durdurur.
local function with_hook(fn)
  debug.sethook(function()
    if py.hard_cancel() then error(CANCEL, 0) end
  end, "", 1000000)
  local r = table.pack(xpcall(fn, traceback))
  debug.sethook()
  return table.unpack(r, 1, r.n)
end

function osi.run_macro(i, lines, sel, active)
  local m = osi.macros[i]
  local subs, state = osi.make_subs(lines, false)
  return state, result_of(with_hook(function() return m.process(subs, sel, active) end))
end

function osi.validate_macro(i, lines, sel, active)
  local m = osi.macros[i]
  if not m.validate then return "ok", true end
  local subs = osi.make_subs(lines, true)
  return result_of(xpcall(function() return m.validate(subs, sel, active) end, traceback))
end

function osi.is_active(i, lines, sel, active)
  local m = osi.macros[i]
  if not m.is_active then return "ok", false end
  local subs = osi.make_subs(lines, true)
  return result_of(xpcall(function() return m.is_active(subs, sel, active) end, traceback))
end

function osi.run_filter(i, lines, config)
  local f = osi.filters[i]
  local subs, state = osi.make_subs(lines, false)
  return state, result_of(with_hook(function() return f.process(subs, config or {}) end))
end

function osi.meta_of(state, t)
  local m = state.meta[t]
  if not m then return nil, nil, nil end
  return m.pristine, m.src, m.slot
end

return osi
