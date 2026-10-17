# Yuzu 柚子

Sıfırdan yazılmış anime altyazı aracı: **Aegisub**'un iş akışı + **Subtitle Edit**'in araçları, Aegisub uyumlu **Lua otomasyonu** ile.

* ASS/SSA tam desteği (stil, katman, karaoke, etiketler), SRT / WebVTT içe-dışa aktarma
* Ses dalga formundan zamanlama, anahtar kareler, libass ile **altyazılı video önizleme** (ffmpeg gerekir)
* Aegisub Lua otomasyonu: `aegisub.register_macro/filter`, `karaskel`, `utils`, `unicode`, `re`, `aegisub.util`, `aegisub.dialog`, DependencyControl uyumluluk katmanı
* QC (CPS/CPL/çakışma/stil/etiket), zamanlama post-işlemcisi, bul-değiştir (regex), yeniden ölçekleme, stil yöneticisi
* Rust çekirdeği: hızlı, tek ikili dosya, Windows/Linux/macOS

## Derleme

```
cd yuzu
cargo build --release -p yuzu-gui -p yuzu-cli
```

`yuzu-gui` arayüzdür, `yuzu` komut satırı aracıdır (`yuzu run betik.lua Makro in.ass out.ass` ile Lua betiklerinizi başsız çalıştırabilirsiniz).

Video/ses için `ffmpeg`, `ffprobe`, `ffplay` PATH'te (veya çalıştırılabilir dosyanın yanında) olmalıdır.

## Lua betikleri

Betikler şu klasörlerden otomatik yüklenir: `~/.config/yuzu/automation/autoload`, `~/.aegisub/automation/autoload`, `%APPDATA%\Aegisub\automation\autoload` ve Yuzu'nun kendi `automation/autoload` klasörü. *Otomasyon → Betik klasörü ekle…* ile yenisini ekleyebilirsiniz. Ayrıntılar: [`automation/README.md`](automation/README.md).

## Kısayollar

| Kısayol | İşlev |
|---|---|
| Ctrl+O / S / Z / Shift+Z | aç / kaydet / geri al / yinele |
| Ctrl+F | bul & değiştir |
| Ctrl+Enter | sonraki satır (sonda yeni satır) |
| Ctrl+D | satırı çoğalt |
| F1 / F2 | satır başlangıcı / sonu = video konumu |
| F3 / F4 | satırı oynat / bitişi oynat |
| Ctrl+Space | oynat/duraklat |
| Ctrl+←/→ | kare geri/ileri (Shift ile 1 sn) |

Sürüm geçmişi: [`CHANGELOG.md`](CHANGELOG.md) — yol haritası: [`ROADMAP.md`](ROADMAP.md).
