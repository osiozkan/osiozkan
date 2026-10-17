# Üçüncü taraf bileşenler

- `osisub/automation/lua/include/{karaskel*.lua, utils*.lua, unicode.lua, re.lua, clipboard.lua, lfs.lua, cleantags.lua}`
  ve `osisub/automation/lua/autoload/*.lua`: Aegisub projesinden (github.com/TypesettingTools/Aegisub),
  dosyaların içindeki BSD/ISC/MIT lisans başlıklarıyla, değiştirilmeden.
- `osisub/automation/lua/include/aegisub/{util,unicode,re}.lua`: Aegisub'ın aynı adlı MoonScript
  modüllerinin düz Lua karşılıkları; özgün telif bildirimleri dosya başlarında.
- Çalışma zamanı bağımlılıkları: PySide6 (LGPL), lupa (MIT, LuaJIT MIT), regex (Apache 2.0), OpenCV (Apache 2.0, hareket takibi), numpy (BSD), PyAV (BSD).
