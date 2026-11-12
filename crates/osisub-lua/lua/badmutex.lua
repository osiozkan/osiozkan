-- osisub: BM.BadMutex karşılığı. Gerçeği ffi ile yerel bir DLL yükler (betikler arası kilit);
-- osisub'da betikler tek iş parçacığında sırayla çalıştığından kilit her zaman alınır.
local BadMutex = { _osisub = true }
local locked = false
function BadMutex.lock() locked = true end
function BadMutex.tryLock()
  if locked then return false end
  locked = true
  return true
end
function BadMutex.unlock() locked = false end
-- DepCtrl kaydı varsa sürümü kaydet
local ok, DepCtrl = pcall(require, "l0.DependencyControl")
if ok and DepCtrl then
  local v = DepCtrl{ name = "BadMutex", version = "0.1.3", moduleName = "BM.BadMutex" }
  return v:register(BadMutex)
end
BadMutex.version = "0.1.3"
return BadMutex
