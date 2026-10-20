-- nightsub: argument checking is not enforced; `check'...'(fn)` returns fn unchanged.
return function() return function(fn) return fn end end
