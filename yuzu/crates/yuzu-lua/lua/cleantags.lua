-- cleantags.lua: merge adjacent override blocks, remove empty ones
local cleantags = {}
function cleantags.clean(text)
  text = text:gsub("}{", "")
  text = text:gsub("{}", "")
  return text
end
function cleantags.cleanup(subs, sel)
  for _, i in ipairs(sel) do
    local l = subs[i]
    l.text = cleantags.clean(l.text)
    subs[i] = l
  end
  return sel
end
return setmetatable(cleantags, { __call = function(_, t) return cleantags.clean(t) end })
