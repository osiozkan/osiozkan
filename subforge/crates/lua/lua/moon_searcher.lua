-- package searcher: `require "x"` also finds x.moon next to every package.path entry.
local searchers = package.loaders or package.searchers
table.insert(searchers, 3, function(name)
	local rel = name:gsub("%.", "/")
	local tried = {}
	for tmpl in package.path:gmatch("[^;]+") do
		local path = tmpl:gsub("%.lua$", ".moon"):gsub("%?", (rel:gsub("%%", "%%%%")))
		local f = io.open(path, "rb")
		if f then
			local src = f:read("*a")
			f:close()
			local code, err = require("moonscript.base").to_lua(src)
			if not code then
				error(("error compiling %s: %s"):format(path, tostring(err)), 0)
			end
			return assert(loadstring(code, "@" .. path))
		end
		tried[#tried + 1] = "\n\tno file '" .. path .. "'"
	end
	return table.concat(tried)
end)
