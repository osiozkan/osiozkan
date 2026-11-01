# SubForge — Kurulum ve Kullanım

## Kurulum
Gereksinimler: Python 3.10+ , `ffmpeg` + `ffprobe` (PATH'te), isteğe bağlı ama önerilen **libass**.

```bash
pip install -r requirements.txt     # PySide6, lupa (LuaJIT), numpy
python -m subforge                  # arayüzü aç
python -m subforge dosya.ass        # dosyayla aç
```

**libass** (birebir ASS önizleme için):
- Linux: `libass9` / `libass-dev` paketi (çoğu dağıtımda kurulu).
- macOS: `brew install libass`.
- Windows: `libass-9.dll` dosyasını `subforge/bin/` klasörüne koyun (ör. MSYS2 `mingw-w64-x86_64-libass` ya da mpv derlemesinden)
  veya `SUBFORGE_LIBASS=C:\yol\libass-9.dll` ortam değişkenini ayarlayın. Yoksa SubForge yaklaşık bir önizleme çizer.

## Lua betikleriniz
Aegisub'ın Auto4 betikleri değişiklik gerektirmeden çalışır (LuaJIT, `aegisub.*`, `karaskel`, `utils`, `re`, `unicode`…).
Betiklerinizi şuraya koyun — açılışta otomatik taranır, **Otomasyon** menüsünde görünür:

| Klasör | Açıklama |
|---|---|
| `scripts/` (depo içinde) | proje ile birlikte sürümlenen betikler |
| `~/.subforge/automation/autoload/` | kişisel betikler |
| `~/.subforge/automation/include/` | `include("x.lua")` ile yüklenen kitaplıklar |
| Otomasyon > Betik klasörü ekle… | herhangi bir klasör (ör. Aegisub'ın `automation/autoload`'ı) |

`Ctrl+Shift+R` = betikleri yeniden yükle. Paketle gelen Aegisub betikleri (Karaoke Templater, Clean Tags, …) hazırdır.
Komut satırından: `python -m subforge lua betik.lua dosya.ass -m "Makro adı" --set alan=değer -o cikti.ass`.

## Çalışma akışı
1. **Dosya > Aç** (ASS/SSA/SRT/VTT/SUB/LRC; kodlama otomatik: UTF-8/16, cp1254), **Medya > Video aç**.
   Ses dalga formu, anahtar kareler ve (varsa) VFR zaman kodları otomatik yüklenir.
2. Ses panelinde: sürükleyerek satır kenarlarını zamanla (kenarlar anahtar kareye/komşu satıra yapışır; **Shift** = yapışma yok),
   boş alanı sürükle → aralık seç → **Enter** aktif satıra uygula / **Shift+Enter** yeni satır, **Ctrl+tekerlek** yakınlaştır.
3. Düzenleme kutusunda metin yazın; **Ctrl+Enter** sonraki satıra geçer (sondaysa yeni satır ekler).
4. Video üzerinde araçlar: ✥ konum, ▭ kırpma (sağ tuş = ters kırpma), ⟳ döndürme.
5. **Araçlar > Kalite kontrol** (CPS, satır uzunluğu, süre, çakışma, boşluk, stil, etiket dengesi). Izgarada sorunlu satırlar renklenir.
6. **Zamanlama** menüsü: kaydır, son işlemci (lead-in/out + anahtar kare yapışması + boşluk kapatma), iki noktalı senkron, FPS dönüşümü.
7. **Karaoke zamanlayıcı** (Ctrl+K): hece böl, eşitle, dokunarak zamanla (video oynarken `C` tuşu).
8. **Dosya > Dışa aktar**: SRT/VTT/TXT/ASS; **Filtreli dışa aktar**: Auto4 filtreleri.

## Önemli kısayollar (Araçlar/Kısayol tuşları… menüsünden değiştirilebilir)
| Tuş | İşlev |
|---|---|
| Ctrl+Z / Ctrl+Y | geri al / yinele |
| Ctrl+F · Ctrl+Shift+F | bul/değiştir · satır seç |
| Ctrl+D · Ctrl+Del · Ctrl+Ins | çoğalt · sil · satır ekle |
| Ctrl+J · Ctrl+B | sonrakiyle birleştir · imleçten böl |
| Ctrl+/ | yorum yap/geri al |
| Boşluk · Ctrl+Boşluk · Ctrl+L | oynat/duraklat · (her durumda) · aktif satırı oynat |
| , / . · Alt+←/→ | önceki/sonraki kare |
| Alt+↑/↓ | önceki/sonraki satır |
| Ctrl+[ / Ctrl+] | başlangıcı/bitişi video konumuna ayarla |
| Ctrl+H · Ctrl+Shift+P | zamanları kaydır · son işlemci |
| Ctrl+Shift+T · Ctrl+K · Ctrl+Shift+Q | stil yöneticisi · karaoke · kalite kontrol |

## Komut satırı
```
python -m subforge convert in.ass out.srt          # biçim dönüştür
python -m subforge check in.ass --max-cps 17       # kalite raporu (hata varsa çıkış kodu 1)
python -m subforge shift in.srt 1500 -o out.srt    # +1.5 sn
python -m subforge fix in.ass --bridge 250         # boşluk kapat, çakışma/yaygın hata düzelt, yineleneni birleştir
python -m subforge lua --list betik.lua            # betikteki makroları listele
python -m subforge info video.mkv                  # ffprobe özeti
```
