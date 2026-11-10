# Kısayollar

| Kısayol | İşlev |
|---|---|
| Ctrl+O / Ctrl+S / Ctrl+Shift+S | Aç / Kaydet / Farklı kaydet |
| Ctrl+Z / Ctrl+Y (Ctrl+Shift+Z) | Geri al / Yinele |
| Ctrl+C / Ctrl+X / Ctrl+V | Kopyala / Kes / Yapıştır (satırlar) |
| Ctrl+D | Çoğalt |
| Ctrl+Enter | Satır ekle (sonra) |
| Ctrl+M | Seçili ardışık satırları birleştir |
| Del | Seçili satırları sil (metin kutusunda değilken) |
| Ctrl+A | Tümünü seç (ızgara) |
| Ctrl+F / Ctrl+H | Bul / Değiştir |
| ↑ ↓ PageUp PageDown | Satır gezinme (metin kutusunda değilken) |
| Alt+↑ / Alt+↓ | Önceki / sonraki satır (her zaman) |
| F5 | Etkin satırı oynat/durdur (video+ses) |
| Shift+F5 | Playhead'den oynat/durdur (satır sınırı olmadan, en çok 10 dk) |
| F6 / F7 | Playhead'i başlangıç / bitiş yap |
| Dalga formu: sürükle (kenar tutamaçları) | Başlangıç/bitiş ayarla (kareye/anahtar kareye yapışır) |
| Dalga formu: Alt+sürükle | Satırı taşı |
| Dalga formu: tıkla / Shift+tıkla / sağ tıkla | Playhead / başlangıç / bitiş |
| Dalga formu: Ctrl+tekerlek | Yakınlaştır (tekerlek: kaydır) |
| Video: Ctrl+tekerlek / orta tuşla sürükle / orta çift tık | Yakınlaştır (imleç etrafında) / kaydır / sıfırla |
| Dalga formu: sarı hece sınırını sürükle | Karaoke hecesinin süresini ayarla (komşu heceyle paylaşır) |
| Karaoke sekmesi: hece metnine `|` yaz | Heceyi böl |
| Ctrl+G | Zamana git |
| , / . | Playhead'i 1 kare geri / ileri (metin kutusunda değilken) |
| Ctrl + / Ctrl - | Yakınlaştır / uzaklaştır (arayüz) |
| Video aracı Kırpma: sağ tık | Kırpmayı kaldır |
| Video aracı Döndürme: Shift+sürükle | 15° adımlarla döndür |
| Karaoke sekmesi → Tuşla zamanla: Boşluk | Çalarken hecenin başlangıcını işaretle |

## Lua makrolarına kısayol atama
*Otomasyon* sekmesinde her makronun yanındaki **"kısayol yok"** düğmesine tıklayın ve tuş birleşimine basın (ör. `Ctrl+Shift+K`). **Esc** iptal eder, **Backspace** kısayolu kaldırır. Atamalar `settings.json` içinde (`macro_hotkeys`) betik adı + makro adıyla saklanır; betik yeniden yüklendiğinde de geçerlidir. Yerleşik bir kısayolu (ör. `Ctrl+S`) ezerseniz durum çubuğunda uyarı görünür. Metin kutusunda yazarken değiştiricisiz harf kısayolları çalışmaz (F tuşları çalışır).

## Yerleşik komutlara kısayol atama
*Yardım → Kısayolları düzenle…*: video karesinde böl, çoğalt + 1 kare kaydır, satırı oynat, başlangıç/bitişi video karesine al, kare/satır gezinme, satırları seç, sahne değişimlerine yapıştır ve video araçları (konum, hareket, kırpma, vektör kırpma, döndürme, 3B döndürme, ölçek) için kendi tuşlarınızı atayın. Atamalar Lua makro kısayollarıyla aynı yerde (`settings.json` → `macro_hotkeys`, betik adı `@komut`) saklanır.
