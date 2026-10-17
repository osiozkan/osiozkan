@echo off
chcp 65001 >nul
cd /d "%~dp0"
echo Whisper (konusmayi yaziya dokme) icin faster-whisper kuruluyor (~200 MB)...
where py >nul 2>nul
if %errorlevel%==0 (
  py -3 -m pip install faster-whisper
  py -3 -m osisub doctor
) else (
  python -m pip install faster-whisper
  python -m osisub doctor
)
echo.
pause
