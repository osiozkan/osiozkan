-- DepCtrl modül düzeni örneği (a-mo.LineCollection taklidi)
local DependencyControl = require("l0.DependencyControl")
local version = DependencyControl{
  name = "LineCollection", version = "1.3.0", description = "test modülü",
  moduleName = "a-mo.LineCollection", url = "https://example.invalid",
  { "json" },
}
local json = version:requireModules()
local LineCollection = {}
function LineCollection.count(subs, sel) return #sel end
function LineCollection.roundtrip(t) return json.decode(json.encode(t)) end
return version:register(LineCollection)
