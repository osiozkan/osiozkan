-- osisub: SubInspector.Inspector karşılığı. Gerçeği libass'i ffi ile yükleyen yerel bir DLL ister
-- (ASSFoundation getLineBounds, Aegisub-Motion vb. kullanır). Burada satırlar osisub'ın libass
-- çizicisiyle (ffmpeg) betiğin başlığı/stilleriyle, video olmadan çizilir ve her kare için
-- görünenlerin sınırları döndürülür: { x, y, w, h, hash, solid } ya da false (boş kare).
local render = require("osisub.render")

local Inspector = { version = 0x000700, version_string = "0.7.2", _osisub = true }
Inspector.__index = Inspector

function Inspector.new(sub, fontDirectory)
  return setmetatable({ sub = sub, fontDirectory = fontDirectory }, Inspector)
end

setmetatable(Inspector, { __call = function(cls, ...) return cls.new(...) end })

-- Aegisub'ın Lua'ya verdiği iki yerel işlev de kullanılabiliyorsa kare zamanları videodan;
-- video yoksa 23.976 kare/sn varsayılır.
local function frame_of(ms)
  local f = aegisub.frame_from_ms and aegisub.frame_from_ms(ms)
  if f then return f end
  return math.floor(ms * 24000 / 1001 / 1000)
end
local function ms_of(f)
  local ms = aegisub.ms_from_frame and aegisub.ms_from_frame(f)
  if ms then return ms end
  return math.floor(f * 1001 * 1000 / 24000 + 0.5)
end

-- Satırların görüneceği anlar: si_exhaustive olan satırda her kare, diğerlerinde ilk an.
local function render_times(lines)
  local seen, times = {}, {}
  local function add(t)
    t = math.floor(t + 0.5)
    if not seen[t] then seen[t] = true; times[#times + 1] = t end
  end
  for _, line in ipairs(lines) do
    local s, e = line.start_time or 0, line.end_time or 0
    if e > s then
      if line.si_exhaustive then
        local f = frame_of(s)
        while true do
          local t = math.max(s, ms_of(f))
          if t >= e then break end
          add(t)
          f = f + 1
        end
      else
        add(s)
      end
    end
  end
  table.sort(times)
  return times
end

-- bounds, times  |  nil, hata
function Inspector:getBounds(lines)
  if type(lines) ~= "table" or #lines == 0 then
    return nil, "SubInspector: getBounds en az bir satır ister"
  end
  local times = render_times(lines)
  if #times == 0 then return {}, {} end
  local bounds, err = render.bounds(lines, times)
  if not bounds then return nil, "SubInspector: " .. tostring(err) end
  return bounds, times
end

-- Uyumluluk: eski sürümlerde başlık güncellemesi gerekiyordu; osisub her çağrıda güncel betiği kullanır.
function Inspector:updateHeader() return true end
function Inspector:updateScriptHeader() return true end

local ok, DepCtrl = pcall(require, "l0.DependencyControl")
if ok and DepCtrl then
  local v = DepCtrl{ name = "SubInspector", version = Inspector.version_string, moduleName = "SubInspector.Inspector" }
  return v:register(Inspector)
end
return Inspector
