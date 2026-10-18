# Değişiklik günlüğü

## 0.1.0 — ilk gece
- Sıfırdan Python/PySide6 mimarisi, ASS çekirdeği, SRT/VTT/TXT dönüştürücüler.
- Aegisub uyumlu Lua otomasyon çalışma zamanı (LuaJIT): `aegisub.*`, `subs`, `karaskel`, `util`, `unicode`, `re`.
- GUI: tablo, düzenleme paneli, libass önizleme, dalga formu, stil yöneticisi, bul/değiştir, QC, otomasyon menüsü.
- CLI: convert / macros / run / check / shift.
- ffmpeg tabanlı medya: video/ses yükleme, libass ile gerçek ASS önizleme, gerçek zamanlı video oynatma (ffmpeg raw pipe + ffplay ses),
  dalga formu, anahtar kare tespiti ve gösterimi.
- Karaoke: otomatik \k (hece/sözcük/karakter), lead-in/out, dalga formunda hece sınırı sürükleme.
- Typesetting: önizlemede \pos / \org / \move / \clip / \iclip yerleştirme.
- ASS araçları: çözünürlük yeniden örnekleme (pos/move/clip/fs/bord/çizim), font denetimi, font gömme/çıkarma, stil içe aktarma.
- Subtitle Edit tarzı: İE notlarını sil, bozuk karakter onarımı, yaygın hata düzeltme, otomatik satır bölme, istatistik, referans altyazı, zamanlama son işlemcisi.
- Tercihler (CPS/CPL, Lua include/autoload), son dosyalar, sürükle-bırak.
