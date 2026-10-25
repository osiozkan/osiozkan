# OsiSub

Anime-focused subtitle editor: Aegisub-compatible ASS editing and **Lua automation**, plus Subtitle Edit-style tooling
(QC, format conversion, batch fixes). See [ROADMAP.md](ROADMAP.md) for status and plans.

```bash
npm install
npm start          # http://localhost:8765
npm test
```

Drop `.ass/.srt/.vtt`, `.lua` automation scripts, or a video/audio file onto the window. Example: `examples/add_prefix.lua`.
