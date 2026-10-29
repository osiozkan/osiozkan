# Geliştirme günlüğü

Her gece çalışması (gece–06:00) buraya bir kayıt ekler. Yeni oturum önce bu dosyayı ve `docs/ROADMAP.md`'yi okumalı, kaldığı yerden devam etmeli. Sonuç hep `claude/vibrant-clarke-2e8jtn` dalına push edilir; PR yalnızca kullanıcı isterse açılır.

## 2026-10-29 (v0.1.0 — ilk gece)
Depo boştu; sıfırdan başlandı.
- `osisub-core`, `osisub-lua`, `osisub-cli` yazıldı; tüm testler geçiyor (core 12, lua 3).
- Aegisub Lua uyumluluğu için LuaJIT (`mlua`, vendored) seçildi: ffi/bit yerleşik, Aegisub ile aynı 5.1 anlambilimi.
- Karar: GUI için egui/eframe önerildi, henüz başlanmadı.
- Not: Kullanıcının kendi Lua betikleri depoda yok; regresyon testi için betikler gerekli (kullanıcıdan istenmeli).
- Not: "Bugüne kadar altyazıda karşılaştığımız sorunlar" geçmişi bu oturumda erişilebilir değildi; depoda kayıt yok. Sorunlar tanımlandıkça `docs/ROADMAP.md`'ye eklenmeli.
- Sıradaki: gerçek yazı tipi ölçüsü (`text_extents`), override etiket ayrıştırıcı, GUI iskeleti.
- (aynı gece) Gerçek yazı tipi ölçümü, `osisub-doc` (Document: seçim, geri al/yinele, birleşen yazma adımları, böl/birleştir/çoğalt/sil, makro sonucu tek geri-al adımı), Lua arka plan çalışanı (iletişim kutuları arayüze iletilir), override etiket ayrıştırıcı, `osisub-gui` iskeleti.
- Ölçüm: 100k satırlık ASS ayrıştırma ~0.17 sn; karaskel ile 100k satır Lua işleme ~2.8 sn (release).
