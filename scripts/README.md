# Lua betikleri

Aegisub Auto4 betiklerinizi (`.lua`) bu klasöre koyun; SubForge açılışta tarar, makrolar **Otomasyon** menüsünde görünür.
Betikler Aegisub'daki gibi `aegisub.register_macro/register_filter`, `include("karaskel.lua")`, `require "aegisub.re"` vb. kullanabilir.
Başka klasörler için: Otomasyon > Betik klasörü ekle… · komut satırı: `python -m subforge lua betik.lua dosya.ass -m "Makro"`.

`ornek-makro.lua` bir başlangıç örneğidir (iletişim kutusu + karaskel + ilerleme).
