-- aegisub.ffi: helpers used by Aegisub's own modules. Only what scripts could reasonably call.
local ffi = require("ffi")
local M = {}
function M.string(p)
  if p == nil then return nil end
  return ffi.string(p)
end
return M
