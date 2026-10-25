# Automation folder

Put your Aegisub Lua scripts in `automation/autoload/` – they are loaded automatically when OsiSub starts
(Automation menu → *Reload all* re-reads them). Libraries your scripts `include("x.lua")` / `require "x"` can live in
`automation/include/` (or next to the script). Extra folders: set `OSISUB_SCRIPTS=/path/a:/path/b`.
