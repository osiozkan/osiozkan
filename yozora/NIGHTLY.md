# Gece kayıtları

Her gece çalışan rutin bu dosyayı günceller: ne yapıldı, ne sırada. Yeni oturum önce **ROADMAP.md** ve bu dosyayı okur.

## 2026-11-04 (1. gece)
- Depo boştu; proje sıfırdan başlatıldı (`yozora/`).
- `yozora-core`: ASS/SSA/SRT/VTT/MicroDVD/TXT, etiket ayrıştırıcı, TPP, QC, geçmiş (17 test).
- `yozora-lua`: LuaJIT üstünde Automation 4 katmanı; `utils`, `karaskel`, `unicode`, `re`; gerçek yazı tipi ölçümü (7 test).
- `yozora-cli`: info / convert / qc / shift / tpp / lua list|run.
- **Sırada**: kullanıcının kendi Lua betikleriyle uyumluluk denemesi (betikler depoya eklenirse `tests/scripts/` altına),
  `lpeg`/`lfs`/`json` modülleri, ardından GUI iskeleti.
