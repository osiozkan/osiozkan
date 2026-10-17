-- aegisub.clipboard — pano erişimi (uygulama Python/Qt tarafında).
local py = __osisub_py
return {
  get = function() return py.clipboard_get() end,
  set = function(s)
    if type(s) ~= "string" then error("clipboard.set: metin bekleniyordu", 2) end
    return py.clipboard_set(s)
  end,
}
