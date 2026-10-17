-- aegisub.lfs — LuaFileSystem alt kümesi (Aegisub lfs.moon ile aynı arayüz).
-- Uygulama Python'dadır: Windows'ta Türkçe karakterli yollar sorunsuz çalışır.

local py = __osisub_py

local function attributes(path, field)
  local mode, mtime, size, err = py.lfs_attributes(path)
  if err then return nil, err end
  if field == "mode" then return mode end
  if field == "modification" then return mtime end
  if field == "size" then return size end
  if field ~= nil then return nil end
  return {mode = mode, modification = mtime, size = size}
end

local function bool_ret(ok, err)
  if ok then return true end
  return nil, err
end

local function dir(path)
  local list, err = py.lfs_dir(path)
  if err then error(err, 2) end
  local i = 0
  local obj = {
    next = function()
      i = i + 1
      return list[i]
    end,
    close = function() end,
  }
  return obj.next, obj
end

return {
  attributes = attributes,
  chdir = function(p) return bool_ret(py.lfs_chdir(p)) end,
  currentdir = function() return py.lfs_currentdir() end,
  dir = dir,
  mkdir = function(p) return bool_ret(py.lfs_mkdir(p)) end,
  rmdir = function(p) return bool_ret(py.lfs_rmdir(p)) end,
  touch = function(p) return bool_ret(py.lfs_touch(p)) end,
}
