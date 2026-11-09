# Yol haritası ve gece günlüğü

> Gece çalışmaları buradan devam eder: her koşu önce bu dosyayı okur, "Sıradaki" listesinden ilerler, bitince günlüğe ekler. Test geçmeden commit yok (`npm test`, `npx tsc --noEmit`, `node e2e/smoke.mjs`).

## Günlük
- **2026-11-09 (Gece 1)** — Depo boştu; sıfırdan: ASS/SRT/VTT çekirdeği, QC, toplu işlemler, Aegisub-uyumlu Lua çalışma ortamı (fengari, LuaJIT sayı biçimi yaması), web arayüzü (ızgara, düzenleme, video önizleme, dalga formu, stil yöneticisi, QC, Lua makro çalıştırma). 14 birim testi + tarayıcı duman testi.

## Sıradaki (öncelik sırasıyla)
1. **Kullanıcının Lua betikleriyle uyumluluk sınaması** — gerçek betikleri (`kara-templater`, `Automation 4 templater`, `cleantags`, `Selection Sort` vb.) `test/fixtures/` içine alıp çalıştır; eksik API'leri tamamla. (Kullanıcıdan betiklerini repoya eklemesi istenecek.)
2. **Gerçek libass işleme**: JASSUB (WASM) entegrasyonu; \t, \move, \clip, \fad, karaoke \kf, çizim (\p) doğru gösterim.
3. **Ses/dalga formu**: büyük dosyalar için Web Worker'da tepe hesabı, spektrogram, otomatik zamanlama (VAD), tuş atamalı hızlı zamanlama (Aegisub tarzı: A/S/D/F…), anahtar kareye yapışma.
4. **Karaoke modu**: hece bölme, \k zamanlama (ses üstünde), şablon (kara-templater) çalıştırma, furigana.
5. **Subtitle Edit özellikleri**: çeviri modu (iki panel), yazım denetimi (hunspell-wasm), OCR'siz görsel altyazı içe aktarım (PGS/VobSub — ağır), toplu dönüştürme, çok sayıda format (SBV, SUB, LRC, TTML, SCC, ASS↔SRT stil eşlemeleri), akıllı satır kırma, birleştir/ayır kuralları.
6. **Performans**: 50k+ satır testi, ızgara sanallaştırma ölçümü, Lua çalıştırmayı Web Worker'a taşı, geri-al yığınını fark (diff) tabanlı yap.
7. **Paketleme**: Tauri/Electron ile masaüstü sürüm, dosya sistemi, yakın zamanlı dosyalar, otomatik kaydetme/kurtarma, `io.open` destekli Lua.
8. **Aegisub uyumluluğu**: `subs` içinde `extradata`, `aegisub.dialog` düğme kimlikleri, `lpeg`/`lfs` benzeri modüller, `aegisub.get_audio_selection`, Aegisub proje dosyası bölümleri (Video File, Active Line…), kısayol/menü özelleştirme.
9. **Arayüz**: bölünebilir paneller, tema, Türkçe/İngilizce yerelleştirme, kısayol editörü, sürükle-bırak.

## Bilinen sınırlar
- Lua 5.3 (fengari) üzerinde çalışır; LuaJIT'e özgü FFI/JIT yok. `string.upper` bayt tabanlıdır (Aegisub ile aynı).
- Metin ölçümü tarayıcı canvas ile yaklaşıktır (libass ile birebir değil); karaskel konumları küçük sapmalı olabilir.
- Önizleme basit DOM'dur (\pos, \an, i/b/u/s, renk, boyut); animasyon etiketleri henüz yok.
