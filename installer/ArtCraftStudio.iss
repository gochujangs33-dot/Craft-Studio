; Inno Setup script: installs the launcher + all 13 Craft apps as ONE program.
; Only the launcher gets shortcuts, so users always start from the launcher screen.

#define AppName "ArtCraft Studio"
#define AppVersion "1.0.0"
#define Dist "..\dist"

[Setup]
AppId={{6F1E2C1A-3D0B-4C59-9A0B-1C2D3E4F5A6B}
AppName={#AppName}
AppVersion={#AppVersion}
AppPublisher=ArtCraft Studio (bundling storytold/* Craft apps)
DefaultDirName={autopf}\ArtCraftStudio
DefaultGroupName={#AppName}
OutputBaseFilename=ArtCraftStudio-Setup-x64
SetupIconFile=..\launcher\assets\app.ico
UninstallDisplayIcon={app}\ArtCraftStudio.exe
Compression=lzma2/ultra64
SolidCompression=yes
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
WizardStyle=modern
DiskSpanning=no

[Tasks]
Name: "desktopicon"; Description: "Create a desktop shortcut"; Flags: unchecked

[Files]
Source: "{#Dist}\ArtCraftStudio.exe"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#Dist}\apps\*"; DestDir: "{app}\apps"; Flags: ignoreversion recursesubdirs createallsubdirs
Source: "{#Dist}\LICENSES\*"; DestDir: "{app}\LICENSES"; Flags: ignoreversion recursesubdirs skipifsourcedoesntexist

[Icons]
Name: "{group}\{#AppName}"; Filename: "{app}\ArtCraftStudio.exe"
Name: "{autodesktop}\{#AppName}"; Filename: "{app}\ArtCraftStudio.exe"; Tasks: desktopicon

[Run]
Filename: "{app}\ArtCraftStudio.exe"; Description: "Launch {#AppName}"; Flags: nowait postinstall skipifsilent
