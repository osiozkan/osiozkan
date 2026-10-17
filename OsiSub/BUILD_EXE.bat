@echo off
chcp 65001 >nul
cd /d "%~dp0"
rem ================================================================
rem  OsiSub .exe paketi (Windows). Bir kez çalıştır; sonuç:
rem    dist\OsiSub\OsiSub.exe            (klasörü istediğin yere taşıyabilirsin)
rem    dist\OsiSub-X.Y.Z-Kurulum.exe     (Inno Setup kuruluysa: tek tıkla kurulum)
rem ================================================================
set PY=py -3
where py >nul 2>nul
if errorlevel 1 set PY=python

echo [1/6] Paketler kuruluyor...
%PY% -m pip install --upgrade pip
%PY% -m pip install -r requirements.txt pyinstaller
if errorlevel 1 goto hata

for /f "delims=" %%v in ('%PY% -c "import osisub;print(osisub.__version__)"') do set VER=%%v
echo Surum: %VER%

echo [2/6] Simge...
%PY% tools\make_icon.py packaging\osisub.ico

echo [3/6] Paketleniyor (birkac dakika surer)...
%PY% -m PyInstaller --noconfirm --clean packaging\osisub.spec
if errorlevel 1 goto hata

echo [4/6] ffmpeg / ffprobe / libmpv kopyalaniyor (bin klasorunden)...
if not exist dist\OsiSub\bin mkdir dist\OsiSub\bin
if exist bin\ffmpeg.exe copy /y bin\ffmpeg.exe dist\OsiSub\bin\ >nul
if exist bin\ffprobe.exe copy /y bin\ffprobe.exe dist\OsiSub\bin\ >nul
if exist bin\libmpv-2.dll copy /y bin\libmpv-2.dll dist\OsiSub\bin\ >nul
if not exist dist\OsiSub\bin\ffmpeg.exe echo   UYARI: bin\ffmpeg.exe yok - KURULUM.md 3b. adim
if not exist dist\OsiSub\bin\libmpv-2.dll echo   UYARI: bin\libmpv-2.dll yok - KURULUM.md 3b. adim
copy /y KURULUM.md dist\OsiSub\ >nul
copy /y README.md dist\OsiSub\ >nul
copy /y CHANGELOG.md dist\OsiSub\ >nul

echo [5/6] Denetim...
dist\OsiSub\OsiSub-konsol.exe doctor

echo [6/6] Kurulum dosyasi (Inno Setup varsa)...
set ISCC=
where iscc >nul 2>nul && set ISCC=iscc
if "%ISCC%"=="" if exist "%ProgramFiles(x86)%\Inno Setup 6\ISCC.exe" set ISCC="%ProgramFiles(x86)%\Inno Setup 6\ISCC.exe"
if "%ISCC%"=="" if exist "%ProgramFiles%\Inno Setup 6\ISCC.exe" set ISCC="%ProgramFiles%\Inno Setup 6\ISCC.exe"
if "%ISCC%"=="" (
  echo   Inno Setup bulunamadi; kurulum dosyasi atlandi. dist\OsiSub klasoru dogrudan calisir.
) else (
  %ISCC% /DAppVer=%VER% packaging\osisub.iss
)
echo.
echo Hazir: dist\OsiSub\OsiSub.exe
pause
exit /b 0

:hata
echo.
echo HATA: paketleme tamamlanamadi. Yukaridaki iletiyi bana gonder.
pause
exit /b 1
