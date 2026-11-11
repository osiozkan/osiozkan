script_name = "DepCtrl Testi"
script_description = "DependencyControl uyumluluk testi"
script_author = "osisub"
script_version = "1.0.2"
script_namespace = "osisub.DepTest"

local DependencyControl = require("l0.DependencyControl")
local dep = DependencyControl{
  feed = "https://example.invalid/feed.json",
  {
    { "a-mo.LineCollection", version = "1.2.0", url = "https://example.invalid" },
    { "olmayan.Modul", optional = true },
    "aegisub.util",
  },
}
local LineCollection, Opt, util = dep:requireModules()
assert(Opt == nil)
assert(type(util.copy) == "function")
assert(LineCollection.version.version == 65536 + 3 * 256)
assert(dep:getVersionString() == "1.0.2", dep.name .. " " .. dep:getVersionString())
assert(DependencyControl:parseVersion("2.1.3") == 2 * 65536 + 256 + 3)

local cfg = dep:getConfigHandler({ kalinlik = 2, ad = "varsayılan" })

dep:registerMacros({
  { "Say", "Seçili satırları sayar", function(subs, sel)
      aegisub.log("sayi=%d\n", LineCollection.count(subs, sel))
      local rt = LineCollection.roundtrip({ a = { 1, 2, 3 }, b = "ğüşİ\n\"x\"", c = true })
      aegisub.log("json=%s,%s,%s\n", rt.a[3], rt.b, tostring(rt.c))
      cfg.c.kalinlik = cfg.c.kalinlik + 1
      cfg:write()
    end },
  { "Ayar", "Ayarı okur", function()
      local c2 = dep:getConfigHandler({ kalinlik = 0 })
      aegisub.log("kalinlik=%d ad=%s\n", c2.c.kalinlik, c2.c.ad)
    end },
})
dep:registerMacro(function() aegisub.log("tek\n") end)

local ok, err = pcall(function()
  local d2 = DependencyControl{ { { "yok.BuModul", url = "https://ornek.invalid/yok" } } }
  d2:requireModules()
end)
assert(not ok and tostring(err):find("yok.BuModul", 1, true) and tostring(err):find("ornek.invalid", 1, true), tostring(err))
