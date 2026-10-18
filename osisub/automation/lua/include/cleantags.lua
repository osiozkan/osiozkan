-- cleantags.lua: minimal tag cleaner (Osisub). Merges adjacent override blocks and drops empty ones.
function cleantags(text)
  local prev
  repeat
    prev = text
    text = text:gsub("}{", "")
  until text == prev
  text = text:gsub("{}", "")
  return text
end
return cleantags
