# Osisub kullanım kılavuzu

## Kurulum
```
pip install -e .          # lupa, PySide6-Essentials, numpy, spylls
sudo apt install ffmpeg hunspell-tr   # video/ses/önizleme ve Türkçe yazım denetimi (isteğe bağlı ama önerilir)
osisub-gui dosya.ass      # veya: python -m osisub dosya.ass
```

## Ekran düzeni
Üstte **önizleme** (libass ile gerçek ASS render) ve **düzenleme paneli**, ortada **dalga formu**, altta **satır tablosu**.
Önizleme: seçilen satırın zamanında kare + altyazı. Video açılmamışsa koyu zemin üzerinde çizilir.

## Kısayollar
| Tuş | İş |
|---|---|
| Ctrl+N / O / S / Shift+S | Yeni / Aç / Kaydet / Farklı kaydet (kaydederken `.bak` yedeği alınır; 2 dk'da bir otomatik kayıt `~/.osisub/autosave`) |
| Ctrl+Z / Ctrl+Y | Geri al / Yinele |
| Ctrl+Enter | Satır ekle · Ctrl+D çoğalt · Ctrl+Del sil · Ctrl+M birleştir · Ctrl+B imleçten böl |
| Ctrl+F | Bul/değiştir · Ctrl+G satıra git · Ctrl+Shift+A desene göre seç |
| Ctrl+P | Seçili satırı çal · Ctrl+Space videoyu oynat/durdur · Esc durdur |
| Alt+←/→ | Bir kare geri/ileri |
| Ctrl+1 / Ctrl+2 | Başlangıç / bitişi oynatma konumuna ata · Ctrl+3 bitişi ata ve sonraki satıra geç |
| Ctrl+L | Önizlemede etkileşim (typesetting) aç/kapat |
| F7 | Kalite kontrol (CPS, satır uzunluğu, çakışma, süre, stil, etiket) · F8 yazım denetimi |

## Dalga formu
Fare tekeri: kaydır · Ctrl+tekerlek: yakınlaştır · tıkla: konum · aktif satırın sarı sınırlarını sürükle: zamanlama ·
karaoke satırında turkuaz hece sınırlarını sürükle: `\k` süreleri (toplam sabit kalır). Yeşil kesik çizgiler anahtar karelerdir.

## Typesetting
Typesetting menüsünden mod seç (`\pos`, `\org`, `\move` iki tıklama, `\clip`/`\iclip` sürükle), önizlemede tıkla/sürükle;
etiket satırın ilk bloğuna yazılır.

## Lua otomasyonu (Aegisub uyumlu)
Betikler `~/.osisub/automation/autoload`, `~/.aegisub/automation/autoload`, `./automation/autoload` ve Tercihler'deki ek
klasörden yüklenir; include klasörleri aynı mantıkla (`include("karaskel.lua")`, `require "ILL.ILL"` vb.). Otomasyon menüsünden
çalıştırın; diyaloglar Qt formu olarak açılır, ilerleme penceresi iptal destekler. Komut satırı:
```
osisub macros betik.lua
osisub run betik.lua giris.ass -o cikis.ass -m 0 -s 1-5
```
Gerçek Aegisub `kara-templater`, `cleantags`, `strip-tags` betikleri değiştirilmeden çalışır.

## Diğer
Çözünürlük yeniden örnekleme, font denetimi/gömme, stil içe aktarma, zamanlama son işlemcisi, otomatik `\k`, lead-in/out,
İE notlarını silme, bozuk karakter onarımı (`Ã¼`→`ü`), SRT/VTT/TXT dışa aktarım (Farklı kaydet).
