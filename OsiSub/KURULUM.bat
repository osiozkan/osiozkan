@echo off
chcp 65001 >nul
cd /d "%~dp0"
echo OsiSub icin gerekli paketler kuruluyor...
where py >nul 2>nul
if %errorlevel%==0 (
  py -3 -m pip install --upgrade pip
  py -3 -m pip install -r requirements.txt
  py -3 -m osisub doctor
) else (
  python -m pip install --upgrade pip
  python -m pip install -r requirements.txt
  python -m osisub doctor
)
echo.
pause
