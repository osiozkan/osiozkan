# Yol haritası

Hedef: Aegisub'ın tüm işlevleri + Subtitle Edit'in iyi özellikleri + eksiksiz Lua script uyumluluğu, kolay arayüz.

## Tamamlandı (gece 1)
- [x] ASS çekirdeği, SRT/VTT dönüştürücü, Lua host, karaskel/utils alt kümesi, CLI
- [x] `unicode` ve `re` (regex) modülleri (gece 1, ikinci tur)

## Sıradaki geceler (öncelik sırasıyla)
1. Lua uyumluluğu: `bit`, `lfs` benzeri modüller; `aegisub.dialog.display` için gerçek arayüz bağlama;
   Aegisub'ın `karaskel`/`utils`/`clipboard`/`cleantags` tam portu; gerçek font ölçümü (`text_extents`)
2. Override tag ayrıştırıcı/yazıcı (`\t`, `\clip`, `\move`, `\fad`...), tag temizleme, stil çakışması çözümü
3. Zamanlama: frame/timecode (VFR), keyframe snapping, lead-in/out, shift, CPS/CPL kontrolleri
4. Subtitle Edit özellikleri: toplu dönüştürme, yazım denetimi, tekrar eden satır/boşluk düzeltme, çeviri modu, OCR içe aktarma
5. Medya: libmpv ile video, ses dalga formu/spektrum, otomatik zamanlama yardımcıları
6. Arayüz: masaüstü GUI (Aegisub benzeri ızgara + video + ses + stil yöneticisi), kısayol sistemi, tema
7. Performans: büyük dosyalar (50k satır) için benchmark, artımlı ayrıştırma, geri al/yinele
8. Anime özel: karaoke şablon motoru (kara-templater uyumlu), romaji/kanji karaoke yardımcıları, tipografi (fx) kitaplığı, font toplayıcı, mkv gömme

## Bilinen sorunlar / notlar
- `text_extents` şimdilik yaklaşık değer döner (karaoke düzeni gerçek font ölçümü gerektirir)
- `dialog.display` headless modda varsayılanları otomatik onaylar
