local clipboard = {}
function clipboard.get() return __yuzu_clip_get and __yuzu_clip_get() or nil end
function clipboard.set(s) if __yuzu_clip_set then return __yuzu_clip_set(s) end end
return clipboard
