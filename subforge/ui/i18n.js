/* SubForge UI translations. English is the source language; Turkish below.
   Text nodes and title/placeholder attributes are translated by exact (trimmed) match, also for
   dynamically created dialogs (MutationObserver). Unknown strings stay English. */
(function (g) {
  "use strict";
  const TR = {
    "New": "Yeni", "Open…": "Aç…", "Browse…": "Gözat…", "Save": "Kaydet", "Save as…": "Farklı kaydet…", "+ Line": "+ Satır",
    "Delete": "Sil", "Duplicate": "Çoğalt", "Comment": "Yorum satırı", "Split": "Böl", "Merge": "Birleştir",
    "Styles…": "Stiller…", "Properties…": "Özellikler…", "Tools…": "Araçlar…", "Recover…": "Kurtar…", "✥ Typeset": "✥ Dizgi",
    "Reference…": "Referans…", "Video…": "Video…", "Checks": "Kontroller", "Karaoke": "Karaoke", "Lua macros": "Lua makroları", "Keys": "Kısayollar",
    "Start": "Başlangıç", "End": "Bitiş", "Layer": "Katman", "Style": "Stil", "Actor": "Konuşmacı", "Effect": "Efekt", "Dur": "Süre", "Text": "Metin",
    "Run checks": "Kontrolleri çalıştır", "No checks run yet.": "Henüz kontrol çalıştırılmadı.", "No issues found.": "Sorun bulunamadı.",
    "CPS ≤": "CPS ≤", "Min ms": "Min ms", "Max chars": "Maks. karakter",
    "Upload .lua…": "Lua yükle…", "Add script from disk…": "Diskten betik ekle…", "Run": "Çalıştır", "Filter": "Filtre",
    "Aegisub Automation 4 macros run on the current selection.": "Aegisub Automation 4 makroları seçili satırlarda çalışır.",
    "Tools": "Araçlar", "Shift (ms)": "Kaydır (ms)", "All": "Tümü", "Frame rate": "Kare hızı", "Convert": "Dönüştür",
    "Snap to frames": "Karelere hizala", "Snap": "Hizala", "Two-point resync": "İki noktalı senkron", "Resync": "Senkronla",
    "Resolution": "Çözünürlük", "Resample (scales styles, tags, drawings)": "Yeniden örnekle (stil, etiket, çizimleri ölçekler)",
    "Timing cleanup": "Zamanlama düzeltme", "Fix overlaps & short lines": "Çakışmaları ve kısa satırları düzelt",
    "Lead in / out": "Giriş / çıkış payı", "Apply": "Uygula", "Chain": "Zincirle", "Enforce": "Uygula", "Sort": "Sırala", "reverse": "ters",
    "Text cleanup": "Metin temizliği", "Fix common errors": "Sık yapılan hataları düzelt",
    "Remove [sounds] / ♪ / SPEAKER:": "[Sesleri] / ♪ / KONUŞMACI: sil", "Strip all tags": "Tüm etiketleri sil",
    "Casing": "Büyük/küçük harf", "Sentence case": "Cümle düzeni", "Title Case": "Başlık Düzeni", "UPPER": "BÜYÜK", "lower": "küçük",
    "Turkish i/İ rules": "Türkçe i/İ kuralları", "Line breaks": "Satır sonları", "Balance long lines": "Uzun satırları dengele",
    "Split events longer than": "Şundan uzun satırları böl:", "Merge neighbours with gap ≤": "Boşluğu şundan az komşuları birleştir:",
    "Find & replace": "Bul ve değiştir", "Replace all": "Tümünü değiştir", "regex": "regex", "ignore case": "büyük/küçük harf duyarsız",
    "Reports": "Raporlar", "Statistics": "İstatistik", "Fonts used": "Kullanılan fontlar", "Close": "Kapat", "Cancel": "İptal",
    "Reference": "Referans", "Merge reference under each line": "Referansı her satırın altına ekle",
    "Add reference as separate (top) lines": "Referansı ayrı (üst) satırlar olarak ekle", "Import styles from reference .ass": "Referans .ass'tan stilleri al",
    "Script properties": "Betik özellikleri", "Styles": "Stiller", "+ Add": "+ Ekle", "Name": "Ad", "Font": "Yazı tipi", "Size": "Boyut",
    "Bold": "Kalın", "Italic": "İtalik", "Underline": "Altı çizili", "Strikeout": "Üstü çizili", "Scale X": "Ölçek X", "Scale Y": "Ölçek Y",
    "Spacing": "Aralık", "Angle": "Açı", "Outline": "Kontur", "Shadow": "Gölge", "Alignment (numpad)": "Hizalama (numpad)",
    "Margin L": "Kenar boşluğu S", "Margin R": "Kenar boşluğu R", "Margin V": "Kenar boşluğu D", "Encoding": "Kodlama",
    "Settings": "Ayarlar", "Individual scripts": "Tek tek betikler", "Extra font folders": "Ek font klasörleri",
    "Automation autoload folders": "Automation otomatik yükleme klasörleri", "Automation include folders": "Automation include klasörleri",
    "Open subtitle file": "Altyazı dosyası aç", "Save as": "Farklı kaydet", "Files": "Dosyalar",
    "Selected line(s): split text into \\k syllables, then fine-tune the durations (centiseconds).": "Seçili satır(lar): metni \\k hecelerine böl, sonra süreleri (santisaniye) ince ayarla.",
    "Auto-split": "Otomatik böl", "Fit to line": "Satıra sığdır", "Remove k tags": "k etiketlerini sil", "romaji/TR syllables": "romaji/TR heceleri", "characters": "karakterler", "words": "kelimeler",
    "Preview of the selected line · “Video…” loads video/audio": "Seçili satırın önizlemesi · “Video…” video/ses yükler",
    "Select a line…": "Bir satır seç…", "Open: auto": "Aç: otomatik", "UTF-8 (BOM)": "UTF-8 (BOM'lu)", "Turkish (1254)": "Türkçe (1254)",
    "Save format": "Kayıt biçimi", "Save encoding": "Kayıt kodlaması", "Spell-check language of the text box": "Metin kutusunun yazım denetimi dili",
    "Open from disk (server-side browser)": "Diskten aç (sunucu tarafı gözatıcı)", "Automation folders, fonts": "Automation klasörleri, fontlar",
    "Open a crash-recovery autosave": "Kurtarma otomatik kaydını aç", "Load a reference/source-language subtitle for translation": "Çeviri için referans/kaynak dil altyazısı yükle",
    "Visual typesetting: drag on the preview to set \\pos, Shift+drag for \\clip": "Görsel dizgi: önizlemede sürükle = \\pos, Shift+sürükle = \\clip",
    "chain gaps ≤": "boşluk ≤ (zincir)", "min gap": "min boşluk", "max": "maks", "chars": "karakter", "find": "bul", "replace": "değiştir",
    "start": "başlangıç", "end": "bitiş", "style": "stil", "actor": "konuşmacı", "layer": "katman", "text": "metin", "duration": "süre",
    "Statistics": "İstatistik", "Apply": "Uygula", "Selected": "Seçili", "Reset": "Sıfırla", "Upload from browser instead…": "Bunun yerine tarayıcıdan yükle…",
    "Add Automation script": "Automation betiği ekle", "Preview": "Önizleme", "unsaved": "kaydedilmedi",
  };
  const LANGS = { tr: TR };
  let lang = "en";
  try { lang = localStorage.getItem("sf_uilang") || ((navigator.language || "").toLowerCase().startsWith("tr") ? "tr" : "en"); } catch (e) {}
  const dict = LANGS[lang];
  const PATTERNS = {
    tr: [[/^Selected \((\d+)\)$/, "Seçili ($1)"], [/^(\d+) lines$/, "$1 satır"], [/^(\d+) selected$/, "$1 seçili"], [/^(\d+) lines? \+/, "$1 satır +"]],
  };
  function tx(s) {
    if (!dict) return null;
    if (dict[s] !== undefined) return dict[s];
    for (const [re, rep] of (PATTERNS[lang] || [])) if (re.test(s)) return s.replace(re, rep);
    return null;
  }
  function node(n) {
    if (!dict) return;
    if (n.nodeType === 3) {
      const t = n.nodeValue, tt = t.trim();
      if (tt) { const r = tx(tt); if (r !== null && r !== tt) n.nodeValue = t.replace(tt, r); }
    } else if (n.nodeType === 1) {
      if (n.tagName === "SCRIPT" || n.tagName === "STYLE" || n.tagName === "TEXTAREA") return;
      for (const a of ["title", "placeholder"]) { if (n.hasAttribute(a)) { const cur = n.getAttribute(a), r = tx(cur); if (r !== null && r !== cur) n.setAttribute(a, r); } }
      for (const c of n.childNodes) node(c);
    }
  }
  g.SFI18n = {
    lang,
    init() {
      if (!dict) return;
      node(document.body);
      new MutationObserver(ms => { for (const m of ms) { m.addedNodes.forEach(node); if (m.type === "characterData") node(m.target); } })
        .observe(document.body, { childList: true, subtree: true, characterData: true });
    },
    set(l) { try { localStorage.setItem("sf_uilang", l); } catch (e) {} location.reload(); },
    t: s => tx(s) || s,
  };
})(window);
