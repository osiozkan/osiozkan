# Değişiklik Günlüğü

## 0.1.1 — aynı gece
- Aegisub'ın gerçek `karaskel`/`utils`/`kara-templater`/`cleantags`/… betikleri pakete eklendi ve testlerde çalışıyor
- Auto4: `parse_karaoke_data`, `set_undo_point`, `script_resolution`, negatif indeks, `ipairs(subs)`, bayt-şeffaf dizeler
- VFR zaman kodları (v1/v2, videodan algılama), karaoke zamanlayıcı (dokunarak zamanla), satır seç, referans/çeviri paneli
- Görsel dizgi (\pos, \clip/\iclip, \frz), yazı tipi toplama/gömme, ekler, istatistikler, kısayol editörü, Auto4 dışa aktarma filtreleri

## 0.1.0 — ilk gece
- Sıfırdan yazıldı: ASS çekirdeği, SRT/VTT/MicroDVD/LRC, Auto4 uyumlu LuaJIT motoru, Qt arayüzü
- Dalga formu/spektrogram, video + libass önizleme, stil yöneticisi, bul/değiştir, zamanlama araçları, QC
- CLI: convert/check/shift/fix/lua/info
- Bayt-şeffaf Lua dizeleri; Türkçe i/İ/ı/I harf dönüşümü
