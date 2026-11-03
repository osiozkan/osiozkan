local clipboard = {}
function clipboard.get() return aegisub.clipboard.get() end
function clipboard.set(s) return aegisub.clipboard.set(s) end
return clipboard
