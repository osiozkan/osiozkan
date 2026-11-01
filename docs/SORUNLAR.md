# Karşılaşılan altyazı sorunları → SubForge çözümü

Bu dosya, geçmişte yaşanan altyazı sorunlarının ve SubForge'daki karşılığının listesidir.
**Not:** Bu depo/oturum öncesindeki sohbet geçmişi otomasyonun erişiminde değil; aşağıdaki liste
Türkçe/anime altyazı iş akışında yaygın sorunlardan derlendi. Sizin yaşadığınız sorunları buraya ekleyin
(`- [ ] sorun — beklenen davranış`); sonraki turlar bunları test + düzeltme olarak ele alır.

| Sorun | SubForge'da |
|---|---|
| Türkçe karakterler bozuk (`Ã§`, `Ä±`, `ÅŸ`) | açılışta kodlama tespiti (UTF-8/16, cp1254) + *Bozuk karakterleri onar* + QC `mojibake` |
| `i`/`I` büyük-küçük harf dönüşümü yanlış (`iI`↔`İı`) | `tr_upper/tr_lower`, *Harf büyüklüğü (TR)* |
| Lua betiği `text:sub()` ile UTF-8 karakter ortasından kesince hata | Lua dizeleri bayt-şeffaf (Aegisub gibi) |
| Betik hata verince dosya yarım kalıyor | makro başarısızsa/iptalse dosya otomatik eski haline döner; başarılıysa tek geri-al adımı |
| Çakışan/üst üste binen satırlar, kısa boşluklar | QC + *Boşlukları kapat* + *Çakışmaları düzelt* |
| Okuma hızı (CPS) yüksek satırlar | canlı kırmızı CPS, QC, *süre sınırları* |
| Anahtar karede kayan zamanlama | anahtar kare yapışması (sürükleme + son işlemci) |
| VFR videoda kare kayması | videodan VFR algılama, v1/v2 zaman kodu |
| Eksik yazı tipi nedeniyle farklı görünüm | *Yazı tipleri* penceresi (eksik tespiti, toplama, gömme) |
| Dosya kaybı | 60 sn'de bir otomatik yedek (`~/.subforge/autosave`), tüm işlemler geri alınabilir |

## Kullanıcı tarafından eklenecekler
- [ ] (buraya yazın)
