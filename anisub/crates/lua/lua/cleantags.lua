-- cleantags.lua: minimal tag cleaner (merges adjacent override blocks).
cleantags = cleantags or {}
function cleantags.cleanup(text)
  local prev
  repeat
    prev = text
    text = text:gsub("}{", "")
  until text == prev
  return (text:gsub("{}", ""))
end
function cleantags.cleanup_line(line) line.text = cleantags.cleanup(line.text) end
