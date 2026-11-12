-- osisub: PT.PreciseTimer karşılığı (gerçeği ffi ile yerel DLL). Duvar saati osisub'dan gelir.
local okn, native = pcall(require, "osisub.time")
if not okn then native = nil end
local now = native and native.clock or os.clock
local PreciseTimer = { _osisub = true }
PreciseTimer.__index = PreciseTimer

function PreciseTimer.new()
  return setmetatable({ start = now() }, PreciseTimer)
end
-- saniye cinsinden geçen süre
function PreciseTimer:timeElapsed() return now() - self.start end
function PreciseTimer.sleep(a, b)
  local ms = type(a) == "number" and a or b
  if native and native.sleep then native.sleep(ms or 0) end
end
setmetatable(PreciseTimer, { __call = function() return PreciseTimer.new() end })

local ok, DepCtrl = pcall(require, "l0.DependencyControl")
if ok and DepCtrl then
  local v = DepCtrl{ name = "PreciseTimer", version = "0.1.6", moduleName = "PT.PreciseTimer" }
  return v:register(PreciseTimer)
end
PreciseTimer.version = "0.1.6"
return PreciseTimer
