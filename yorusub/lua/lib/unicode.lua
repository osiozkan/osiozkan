local unicode = {}
local function chars(s) local t = {} for c in s:gmatch("[%z\1-\127\194-\244][\128-\191]*") do t[#t+1] = c end return t end
function unicode.chars(s) local i, t = 0, chars(s) return function() i = i + 1 return t[i] end end
function unicode.len(s) return #chars(s) end
function unicode.codepoint(c) return _Y.codepoint(c) end
function unicode.to_upper(s) return _Y.upper(s) end
function unicode.to_lower(s) return _Y.lower(s) end
function unicode.to_fold_case(s) return _Y.lower(s) end
function unicode.charwidth(c) return 1 end
unicode.uc = unicode
_G.unicode = unicode
return unicode
