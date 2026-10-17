; OsiSub kurulum betiği (Inno Setup 6). BUILD_EXE.bat çağırır:  iscc /DAppVer=<surum> packaging\osisub.iss
; Yönetici izni İSTEMEZ (kullanıcının kendi Programlar klasörüne kurar).
#ifndef AppVer
  #define AppVer "1.3.0"
#endif

[Setup]
AppId={{6B3E2E7A-0C1D-4F3B-9A51-05151A0C0510}
AppName=OsiSub
AppVersion={#AppVer}
AppPublisher=Osi
DefaultDirName={localappdata}\Programs\OsiSub
DefaultGroupName=OsiSub
PrivilegesRequired=lowest
OutputDir=..\dist
OutputBaseFilename=OsiSub-{#AppVer}-Kurulum
SetupIconFile=osisub.ico
UninstallDisplayIcon={app}\OsiSub.exe
Compression=lzma2/max
SolidCompression=yes
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
ChangesAssociations=yes
WizardStyle=modern

[Languages]
Name: "turkish"; MessagesFile: "compiler:Languages\Turkish.isl"

[Tasks]
Name: "desktopicon"; Description: "Masaüstüne kısayol koy"; Flags: unchecked
Name: "assoc"; Description: ".ass dosyalarını OsiSub ile açılabilir yap (Birlikte aç listesine ekler)"; Flags: unchecked

[Files]
Source: "..\dist\OsiSub\*"; DestDir: "{app}"; Flags: recursesubdirs createallsubdirs ignoreversion

[Icons]
Name: "{group}\OsiSub"; Filename: "{app}\OsiSub.exe"
Name: "{group}\OsiSub ortam denetimi"; Filename: "{app}\OsiSub-konsol.exe"; Parameters: "doctor"
Name: "{group}\OsiSub'ı kaldır"; Filename: "{uninstallexe}"
Name: "{autodesktop}\OsiSub"; Filename: "{app}\OsiSub.exe"; Tasks: desktopicon

[Registry]
Root: HKCU; Subkey: "Software\Classes\.ass\OpenWithProgids"; ValueType: string; ValueName: "OsiSub.ass"; ValueData: ""; Flags: uninsdeletevalue; Tasks: assoc
Root: HKCU; Subkey: "Software\Classes\OsiSub.ass"; ValueType: string; ValueName: ""; ValueData: "ASS altyazı (OsiSub)"; Flags: uninsdeletekey; Tasks: assoc
Root: HKCU; Subkey: "Software\Classes\OsiSub.ass\DefaultIcon"; ValueType: string; ValueName: ""; ValueData: "{app}\OsiSub.exe,0"; Tasks: assoc
Root: HKCU; Subkey: "Software\Classes\OsiSub.ass\shell\open\command"; ValueType: string; ValueName: ""; ValueData: """{app}\OsiSub.exe"" ""%1"""; Tasks: assoc

[Run]
Filename: "{app}\OsiSub.exe"; Description: "OsiSub'ı başlat"; Flags: nowait postinstall skipifsilent
