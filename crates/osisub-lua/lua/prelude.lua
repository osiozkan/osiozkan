-- osisub Lua ön yükleme: betik ortamı, include, aegisub.log.
local G = _G
local N = __osisub_native
__osisub_native = nil

function __new_script_env(script_dir)
  local env = setmetatable({}, { __index = G })
  env._G = env
  env.include = function(name)
    local src, chunk = N.find_include(name, script_dir)
    local f, err = loadstring(src, "@" .. chunk)
    if not f then error(err, 0) end
    setfenv(f, env)
    return f()
  end
  return env
end

-- Aegisub gibi: izleme düzeyinden (varsayılan 3) yüksek iletiler gösterilmez.
-- 0 ölümcül, 1 hata, 2 uyarı, 3 ileti, 4 ayrıntı, 5 izleme. OSISUB_TRACE_LEVEL ile değiştirilir.
__osisub_trace_level = tonumber(os.getenv("OSISUB_TRACE_LEVEL") or "") or 3
function aegisub.log(...)
  local args = { ... }
  local level = 0
  if type(args[1]) == "number" then level = table.remove(args, 1) end
  if level > __osisub_trace_level then return end
  local msg = tostring(args[1] == nil and "" or args[1])
  if #args > 1 then
    local ok, r = pcall(string.format, unpack(args))
    if ok then msg = r end
  end
  N.log(level, msg)
end
-- Aegisub: aegisub.debug.out(seviye?, biçim, ...) aegisub.log ile aynı anlambilime sahiptir.
aegisub.debug = { out = aegisub.log }

package.preload["aegisub.util"] = function()
  local src, chunk = N.find_include("utils.lua", "")
  local env = setmetatable({}, { __index = G })
  local f = assert(loadstring(src, "@" .. chunk))
  setfenv(f, env)
  f()
  return env.util
end
package.preload["re"] = function() return aegisub.__re end
package.preload["aegisub.re"] = package.preload["re"]
package.preload["unicode"] = function() return aegisub.__unicode end
package.preload["aegisub.unicode"] = package.preload["unicode"]
package.preload["aegisub.clipboard"] = function() return aegisub.__clipboard end
package.preload["lfs"] = function() return aegisub.__lfs end
package.preload["clipboard"] = package.preload["aegisub.clipboard"]
package.preload["utf8"] = function()
  local src, chunk = N.find_include("utf8.lua", "")
  local f = assert(loadstring(src, "@" .. chunk))
  return f()
end

-- require: gömülü modüller (lpeg, moonscript.*) ve include klasörlerindeki .lua/.moon modülleri.
-- Aegisub gibi .moon dosyaları MoonScript ile derlenip yüklenir.
function __osisub_moon_to_lua(src, chunk)
  local moon = require("moonscript.base")
  local code, err = moon.to_lua(src)
  if not code then error(chunk .. ": " .. tostring(err), 0) end
  return code
end
local function searcher(name)
  local src = N.embedded_module(name)
  if src then
    local f, err = loadstring(src, "@osisub/" .. name)
    if not f then error(err, 0) end
    return f
  end
  local s, path = N.find_module(name)
  if not s then return "\n\tosisub include klasörlerinde yok: " .. name end
  if path:sub(-5) == ".moon" then s = __osisub_moon_to_lua(s, path) end
  local f, err = loadstring(s, "@" .. path)
  if not f then error(err, 0) end
  return f
end
table.insert(package.loaders, 2, searcher)

-- LuaJIT'te ipairs(userdata) hata verir; `subs` üzerinde yinelemeyi de destekle.
local real_ipairs = ipairs
function ipairs(t)
  if type(t) == "userdata" then
    return function(_, i)
      i = i + 1
      local v = t[i]
      if v ~= nil then return i, v end
    end, t, 0
  end
  return real_ipairs(t)
end

-- Aegisub unicode modülü: charwidth/chars (bayt tabanlı UTF-8, Aegisub ile aynı)
do
  local u = aegisub.__unicode
  if u then
    function u.charwidth(s, i)
      local b = s:byte(i or 1)
      if not b then return 1
      elseif b < 128 then return 1
      elseif b < 224 then return 2
      elseif b < 240 then return 3
      else return 4 end
    end
    function u.chars(s)
      local curchar, i = 0, 1
      return function()
        if i > #s then return nil end
        local j = i
        curchar = curchar + 1
        i = i + u.charwidth(s, i)
        return s:sub(j, i - 1), curchar
      end
    end
  end
end
