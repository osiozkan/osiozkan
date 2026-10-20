# Third-party notices

`crates/nsub-lua/src/lua/include/*.lua` are Aegisub's stock Automation 4 include files (utils, karaskel, cleantags,
unicode/re/lfs/clipboard loaders), bundled unmodified so existing scripts behave exactly as under Aegisub.
They are © their respective authors (Aegisub Project, Niels Martin Hansen, Rodrigo Braz Monteiro, Thomas Goyne, …) and
are distributed under the BSD / ISC licences reproduced in `crates/nsub-lua/src/lua/LICENSE-Aegisub.txt` and in each file header.

`crates/nsub-lua/src/lua/aegisub/*.lua` are original re-implementations of Aegisub's `aegisub.*` MoonScript modules
(util, unicode, re, lfs) written against the published API and behaviour.
