-- Minimal tag cleaner (merges adjacent override blocks, removes empty ones)
local M = {}
function M.cleanup(text)
  local prev
  repeat prev = text; text = text:gsub("}{", "") until text == prev
  text = text:gsub("{}", "")
  return text
end
return M
