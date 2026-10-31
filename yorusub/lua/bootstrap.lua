-- YoruSub Lua önyükleme: Aegisub uyumlu global ortam. `_Y` Python köprüsüdür.
local Y = _Y

-- ---------- subs nesnesi (newproxy: LuaJIT'te # için __len gerekir) ----------
local function copyline(t)
  local r = {}
  for k, v in pairs(t) do
    if type(v) == "table" then local c = {} for a, b in pairs(v) do c[a] = b end r[k] = c else r[k] = v end
  end
  return r
end
function Y.make_subs(L)
  local p = newproxy(true)
  local mt = getmetatable(p)
  local methods = {
    insert = function(idx, ...) for i, ln in ipairs({...}) do table.insert(L, idx + i - 1, copyline(ln)) end end,
    append = function(...) for _, ln in ipairs({...}) do table.insert(L, copyline(ln)) end end,
    delete = function(...)
      local a = {...}; table.sort(a, function(x, y) return x > y end)
      local last
      for _, i in ipairs(a) do if i ~= last then table.remove(L, i) end last = i end
    end,
    deleterange = function(a, b) for i = b, a, -1 do table.remove(L, i) end end,
    script_resolution = function()
      local w, h = 384, 288
      for _, l in ipairs(L) do
        if l.class == "info" then
          if l.key == "PlayResX" then w = tonumber(l.value) or w end
          if l.key == "PlayResY" then h = tonumber(l.value) or h end
        end
      end
      return w, h
    end,
  }
  mt.__len = function() return #L end
  mt.__index = function(_, k)
    if type(k) == "number" then
      if k < 1 or k > #L then error("subs[" .. k .. "] aralık dışında", 2) end
      return copyline(L[k])
    elseif k == "n" then return #L end
    return methods[k]
  end
  mt.__newindex = function(_, k, v)
    if type(k) ~= "number" then error("subs'a alan atanamaz", 2) end
    if k == 0 then table.insert(L, copyline(v))
    elseif v == nil then table.remove(L, k)
    else L[k] = copyline(v) end
  end
  return p
end

-- ---------- aegisub ----------
aegisub = {
  macros = {}, filters = {}, generators = {},
  progress = {
    set = function(p) Y.progress(p) end,
    task = function(t) Y.task(t) end,
    title = function(t) Y.title(t) end,
    is_cancelled = function() return Y.cancelled() end,
  },
  debug = {
    out = function(lvl, fmt, ...)
      if type(lvl) == "string" then Y.log(string.format(lvl, fmt, ...)) return end
      if select("#", ...) > 0 then Y.log(string.format(fmt, ...)) else Y.log(tostring(fmt)) end
    end,
  },
  dialog = {
    display = function(dlg, btns, ids) return Y.dialog(dlg, btns, ids) end,
    open = function(...) return Y.fileopen(...) end,
    save = function(...) return Y.filesave(...) end,
  },
  lua_supported_version = 4,
  cancel = function() error("\0aegisub.cancel") end,
}
aegisub.log = function(...)
  local n = select("#", ...)
  local a = {...}
  if type(a[1]) == "number" and n >= 2 then table.remove(a, 1) end
  if #a > 1 then Y.log(string.format(unpack(a))) else Y.log(tostring(a[1])) end
end
aegisub.gettext = function(s) return s end
aegisub.decode_path = function(p) return Y.decode_path(p) end
aegisub.text_extents = function(style, text) return Y.text_extents(style, text) end
aegisub.frame_from_ms = function(ms) return Y.frame_from_ms(ms) end
aegisub.ms_from_frame = function(f) return Y.ms_from_frame(f) end
aegisub.video_size = function() return Y.video_size() end
aegisub.keyframes = function() return Y.keyframes() end
aegisub.project_properties = function() return Y.project_properties() end
aegisub.get_audio_selection = function() return nil end
aegisub.clipboard = { get = function() return Y.clip_get() end, set = function(s) Y.clip_set(s) end }

function aegisub.register_macro(name, desc, fn, validate, active)
  table.insert(aegisub.macros, {name = name, description = desc, fn = fn, validate = validate, is_active = active})
end
function aegisub.register_filter(name, desc, prio, fn, cfg)
  table.insert(aegisub.filters, {name = name, description = desc, priority = prio, fn = fn, config = cfg})
end
function aegisub.register_generator(name, desc, fn)
  table.insert(aegisub.generators, {name = name, description = desc, fn = fn})
end

-- ---------- include / require ----------
function include(name)
  local src, path = Y.find_include(name)
  if not src then error("include: dosya bulunamadı: " .. tostring(name), 2) end
  local f, err = loadstring(src, "@" .. path)
  if not f then error(err, 2) end
  return f()
end
local real_require = require
local mods = {}
local function find_mod(name)
  local src, path = Y.find_include(name:gsub("%.", "/") .. ".lua")
  return src, path
end
function require(name)
  if package.loaded[name] ~= nil then return package.loaded[name] end
  if name == "re" then package.loaded.re = Y.re_module(); return package.loaded.re end
  local ok, res = pcall(real_require, name)
  if ok then return res end
  local src, path = find_mod(name)
  if not src then error("module '" .. name .. "' bulunamadı", 2) end
  local f, err = loadstring(src, "@" .. path)
  if not f then error(err, 2) end
  local r = f(name)
  if r == nil then r = package.loaded[name] end
  if r == nil then r = true end
  package.loaded[name] = r
  return r
end

-- Aegisub'de global olan yardımcılar
function string.headtail(s) local a, b = s:match("^(%S*)%s*(.*)$"); return a, b end
if not table.maxn then table.maxn = function(t) local m = 0 for k in pairs(t) do if type(k)=="number" and k>m then m=k end end return m end end
