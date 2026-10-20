-- lfs subset on top of the host-provided primitives
local lfs = {}
lfs.currentdir = function() return __yuzu_fs("cwd") end
lfs.attributes = function(path, attr)
  local t = __yuzu_fs("stat", path)
  if not t then return nil, "cannot obtain information from file '" .. tostring(path) .. "'" end
  if attr then return t[attr] end
  return t
end
lfs.mkdir = function(p) return __yuzu_fs("mkdir", p) end
lfs.rmdir = function(p) return __yuzu_fs("rmdir", p) end
lfs.dir = function(p)
  local list = __yuzu_fs("dir", p) or {}
  local i = 0
  return function() i = i + 1; return list[i] end
end
return lfs
