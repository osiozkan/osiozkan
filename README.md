# Kitsune

Aegisub uyumlu, Subtitle Edit'in kullanışlı özelliklerini içeren anime altyazı editörü (PySide6 + LuaJIT).

```
pip install -e .[dev]
python -m kitsune gui [dosya.ass]
python -m kitsune run bolum.ass automation/example_macros.lua --macro "Kitsune/Etiketleri temizle" --select 3,4
python -m kitsune convert bolum.ass bolum.srt
python -m kitsune lint bolum.ass
pytest
```

Lua makroları (`aegisub.register_macro`) Aegisub ile aynı API'yi kullanır; `automation/` klasörüne koyun.
Durum ve yol haritası: [ROADMAP.md](ROADMAP.md).
