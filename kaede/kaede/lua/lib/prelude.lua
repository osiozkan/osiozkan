-- Kaede: Aegisub automation API (v4) prelude. Receives table K of host callbacks.
return function(K, libdir)
  local aegisub = {}
  _G.aegisub = aegisub
  aegisub.lua_automation_version = 4
  aegisub.__kaede = true

  -- ---------------------------------------------------------------- logging
  local function fmtmsg(...)
    local a = {...}
    local level = 1
    if type(a[1]) == "number" and #a >= 1 and type(a[2]) == "string" then level = table.remove(a, 1) end
    local msg = a[1]
    if msg == nil then msg = "" end
    if #a > 1 then
      local ok, r = pcall(string.format, unpack(a))
      msg = ok and r or tostring(msg)
    else msg = tostring(msg) end
    return level, msg
  end
  function aegisub.log(...) local l, m = fmtmsg(...) K.log(l, m) end
  aegisub.debug = { out = function(...) local l, m = fmtmsg(...) K.log(l, m) end }
  function aegisub.gettext(s) return s end
  aegisub.cancel = function() error("__AEGISUB_CANCEL__", 0) end
  aegisub.progress = {
    set = function(n) K.progress("set", n) end,
    task = function(...) K.progress("task", string.format(...)) end,
    title = function(...) K.progress("title", string.format(...)) end,
    is_cancelled = function() return K.progress("cancelled") and true or false end,
  }

  -- ------------------------------------------------------------ registration
  aegisub.register_macro = function(name, desc, fn, validate, is_active)
    K.register_macro(name, desc, fn, validate, is_active)
  end
  aegisub.register_filter = function(name, desc, prio, fn, dlg)
    K.register_filter(name, desc, prio, fn, dlg)
  end

  -- ------------------------------------------------------------- environment
  aegisub.frame_from_ms = function(ms) return K.frame_from_ms(ms) end
  aegisub.ms_from_frame = function(f) return K.ms_from_frame(f) end
  aegisub.video_size = function() return K.video_size() end
  aegisub.keyframes = function() return K.keyframes() end
  aegisub.project_properties = function() return K.project_properties() end
  aegisub.get_audio_selection = function() return K.audio_selection() end
  aegisub.set_undo_point = function(desc) K.undo_point(desc) end
  aegisub.decode_path = function(p) return K.decode_path(p) end
  aegisub.file_name = function() return K.file_name() end
  aegisub.text_extents = function(style, text)
    if type(style) ~= "table" then error("text_extents: style must be a style table", 2) end
    return K.text_extents(style, text)
  end
  aegisub.clipboard = {
    get = function() return K.clipboard_get() end,
    set = function(s) return K.clipboard_set(s) end,
  }
  aegisub.dialog = {
    display = function(dlg, buttons, ids) return K.dialog(dlg, buttons, ids) end,
    open = function(...) return K.file_dialog("open", ...) end,
    save = function(...) return K.file_dialog("save", ...) end,
  }
  aegisub.io = {}

  -- --------------------------------------------------------------- subtitles
  local function wrap_subs()
    local subs = newproxy(true)   -- userdata so __len works under LuaJIT (like Aegisub)
    local meta = getmetatable(subs)
    meta.__index = function(t, k)
      if type(k) == "number" then
        if k < 1 or k > K.sub_count() or k ~= math.floor(k) then return nil end
        return K.sub_get(k)
      elseif k == "n" then return K.sub_count() end
      return meta.methods[k]
    end
    meta.__newindex = function(t, k, v)
      if type(k) ~= "number" then error("cannot set non-numeric subtitle index", 2) end
      if v == nil then K.sub_delete({k}) else K.sub_set(k, v) end
    end
    meta.__len = function() return K.sub_count() end
    local function strip(self, ...)
      if self == subs then return ... end
      return self, ...
    end
    meta.methods = {
      insert = function(...)
        local a = {strip(...)}
        local idx = table.remove(a, 1)
        K.sub_insert(idx, a)
      end,
      append = function(...) K.sub_insert(K.sub_count() + 1, {strip(...)}) end,
      delete = function(...)
        local a = {strip(...)}
        if type(a[1]) == "table" then a = a[1] end
        K.sub_delete(a)
      end,
      deleterange = function(...)
        local a, b = strip(...)
        local t = {}
        for i = a, b do t[#t + 1] = i end
        K.sub_delete(t)
      end,
      -- Kaede extra: fast path to read all dialogue; not in Aegisub
      __kaede_len = function() return K.sub_count() end,
    }
    return subs
  end
  aegisub.__wrap_subs = wrap_subs

  -- ---------------------------------------------------------------- includes
  local sep = package.config:sub(1, 1)
  package.path = table.concat({ libdir .. "/?.lua", K.script_dir() .. "/?.lua", K.script_dir() .. "/include/?.lua",
    K.user_include() .. "/?.lua", package.path }, ";")
  function include(name)
    local fn = package.searchpath and package.searchpath((name:gsub("%.lua$", "")), package.path)
    if not fn then error("Lua include not found: " .. tostring(name), 2) end
    local f, err = loadfile(fn)
    if not f then error(err, 2) end
    local r = f()
    local base = name:gsub("%.lua$", ""):match("([^/\\]+)$")
    if r ~= nil and rawget(_G, base) == nil then _G[base] = r end
    package.loaded[base] = package.loaded[base] or r
    return r
  end

  -- unicode / re modules implemented in Python
  package.loaded["aegisub.re"] = K.re
  package.loaded["re"] = K.re
  package.loaded["aegisub.unicode"] = K.unicode
  package.loaded["unicode"] = K.unicode
  package.loaded["aegisub.util"] = nil
  package.preload["aegisub.util"] = function() return require("util") end
  package.preload["aegisub"] = function() return aegisub end
  re = K.re
  unicode = K.unicode
  return aegisub
end
