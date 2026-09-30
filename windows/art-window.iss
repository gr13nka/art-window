; Inno Setup script for Art Window
; A daily artwork on your desktop, always fit to the screen.

#ifndef Version
#define Version "0.1.2"
#endif

#ifndef Exe
#error /DExe must be set to the path of art-window.exe
#endif

#ifndef OutputDir
#define OutputDir "target\dist"
#endif

[Setup]
; Privileges and installation directory
PrivilegesRequired=lowest
DefaultDirName={localappdata}\Programs\Art Window
DisableProgramGroupPage=yes

; Application identity
; This GUID must remain constant across versions for upgrades to work
AppId={{D21DFA24-C1CC-489B-8039-AC0EC7A71C0E}
AppName=Art Window
AppVersion={#Version}
AppPublisher=Art Window

; Installer output
OutputBaseFilename=Art-Window-{#Version}-windows-x64-setup
OutputDir={#OutputDir}

; Setup appearance and behavior
SetupIconFile=art-window.ico
UninstallDisplayIcon={app}\art-window.exe
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
WizardStyle=modern
CloseApplications=yes


[Files]
; Install the Art Window executable
Source: "{#Exe}"; DestDir: "{app}"; Flags: ignoreversion

[Icons]
; Create start menu shortcuts with AppUserModelID for proper taskbar pinning
Name: "{autoprograms}\Art Window"; Filename: "{app}\art-window.exe"; AppUserModelID: dev.artwindow

[Tasks]
; Autostart task (checked by default)
Name: autostart; Description: "Start Art Window when I sign in"; Flags: checkedonce

[Registry]
; Only write to registry if autostart task is selected
; The app toggles this value from its tray menu, so the key name must match exactly
Root: HKCU; Subkey: "Software\Microsoft\Windows\CurrentVersion\Run"; \
    ValueType: string; ValueName: "ArtWindow"; ValueData: """{app}\art-window.exe"""; \
    Flags: uninsdeletevalue; Tasks: autostart

[Run]
; Launch the app after installation
Filename: "{app}\art-window.exe"; Flags: nowait postinstall skipifsilent; Description: "Start Art Window"

[UninstallRun]
; Clean shutdown before uninstall
; Flags: runhidden means the command runs without showing a window
; RunOnceId prevents trying to quit multiple times if the app is already closed
Filename: "{app}\art-window.exe"; Parameters: "--quit"; Flags: runhidden; RunOnceId: "QuitArtWindow"
; The tray's Start at login row can have written the Run value without the setup
; task ever being ticked, and then uninsdeletevalue above knows nothing of it.
Filename: "{sys}\reg.exe"; Parameters: "delete HKCU\Software\Microsoft\Windows\CurrentVersion\Run /v ArtWindow /f"; Flags: runhidden; RunOnceId: "ForgetArtWindowLogin"

; %APPDATA%\ArtWindow and %LOCALAPPDATA%\ArtWindow are deliberately left behind, as
; on macOS and Linux: settings and favourites outlive the program.
