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

function aegisub.log(...)
  local args = { ... }
  local level = 0
  if type(args[1]) == "number" then level = table.remove(args, 1) end
  local msg = tostring(args[1] == nil and "" or args[1])
  if #args > 1 then
    local ok, r = pcall(string.format, unpack(args))
    if ok then msg = r end
  end
  N.log(level, msg)
end

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
