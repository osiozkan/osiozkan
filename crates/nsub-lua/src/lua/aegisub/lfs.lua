-- aegisub.lfs: LuaFileSystem subset backed by native helpers.
local M = {}

function M.attributes(path, field)
  local mode, mtime, size = __nsub_fs("stat", path)
  if not mode then return nil, "cannot obtain information from file '" .. path .. "'" end
  if field == "mode" then return mode
  elseif field == "modification" then return mtime
  elseif field == "size" then return size end
  return { mode = mode, modification = mtime, size = size }
end
function M.currentdir() return __nsub_fs("cwd") end
function M.chdir(p) return __nsub_fs("chdir", p) end
function M.mkdir(p) return __nsub_fs("mkdir", p) end
function M.rmdir(p) return __nsub_fs("rmdir", p) end
function M.touch(p) return __nsub_fs("touch", p) end
function M.dir(path)
  local names = __nsub_fs("list", path)
  if not names then error("cannot open " .. path, 2) end
  local i = 0
  local iter = { close = function() end }
  function iter.next()
    i = i + 1
    return names[i]
  end
  return iter.next, iter
end

return M
