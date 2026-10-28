; ==============================================================================
; USB Master Pro — Production Inno Setup Installer Script
; Owned & Developed by BhartX Ecosystems Pvt Ltd (https://bhartx.in)
; Zero-Restart Invariant | Clean Control Panel Registration | x64 Native
; ==============================================================================

#define MyAppName "USB Master Pro"
#define MyAppVersion "1.0.0"
#define MyAppPublisher "BhartX Ecosystems Pvt Ltd"
#define MyAppURL "https://bhartx.in"
#define MyAppSupportEmail "admin@bhartx.in"
#define MyAppExeName "USBMasterPro.exe"

[Setup]
AppId={{D37E881B-74B8-43C6-9BC2-0A51D165E084}
AppName={#MyAppName}
AppVersion={#MyAppVersion}
AppPublisher={#MyAppPublisher}
AppPublisherURL={#MyAppURL}
AppSupportURL={#MyAppURL}
AppUpdatesURL={#MyAppURL}
DefaultDirName={autopf}\BhartX Ecosystems\USB Master Pro
DefaultGroupName=BhartX Ecosystems\USB Master Pro
AllowNoIcons=yes
OutputDir=..\dist\installer
OutputBaseFilename=USBMasterPro-Setup-v{#MyAppVersion}
SetupIconFile=..\ui\assets\favicon.ico
Compression=lzma2/ultra64
SolidCompression=yes
WizardStyle=modern
PrivilegesRequired=admin
ArchitecturesInstallIn64BitMode=x64
RestartIfNeededByRun=no
CloseApplications=force
UninstallDisplayName={#MyAppName} ({#MyAppPublisher})
UninstallDisplayIcon={app}\bin\{#MyAppExeName}

[Languages]
Name: "english"; MessagesFile: "compiler:Default.isl"

[Tasks]
Name: "desktopicon"; Description: "{cm:CreateDesktopIcon}"; GroupDescription: "{cm:AdditionalIcons}"; Flags: unchecked

[Files]
; Main Executables
Source: "..\dist\bin\USBMasterPro.exe"; DestDir: "{app}\bin"; Flags: ignoreversion
Source: "..\dist\bin\USBMasterDaemon.exe"; DestDir: "{app}\bin"; Flags: ignoreversion

; Embedded Drivers & Signing Tools
Source: "..\drivers\*"; DestDir: "{app}\drivers"; Flags: ignoreversion recursesubdirs createallsubdirs
Source: "..\scripts\*"; DestDir: "{app}\scripts"; Flags: ignoreversion recursesubdirs createallsubdirs

; Assets for shortcuts
Source: "..\ui\assets\favicon.ico"; DestDir: "{app}\ui\assets"; Flags: ignoreversion
Source: "..\ui\assets\logo.png"; DestDir: "{app}\ui\assets"; Flags: ignoreversion

[Icons]
Name: "{group}\{#MyAppName}"; Filename: "{app}\bin\{#MyAppExeName}"; IconFilename: "{app}\ui\assets\favicon.ico"
Name: "{group}\Uninstall {#MyAppName}"; Filename: "{uninstallexe}"
Name: "{autodesktop}\{#MyAppName}"; Filename: "{app}\bin\{#MyAppExeName}"; Tasks: desktopicon; IconFilename: "{app}\ui\assets\favicon.ico"

[Run]
; Dynamic PnP Driver Registration (Zero-Reboot Invariant)
Filename: "{sys}\pnputil.exe"; Parameters: "/add-driver ""{app}\drivers\vhci\anyusb_vhci.inf"" /install"; Flags: runhidden; StatusMsg: "Configuring Virtual USB Host Controller (Zero-Reboot)..."
Filename: "{sys}\pnputil.exe"; Parameters: "/add-driver ""{app}\drivers\filter\anyusb_filter.inf"" /install"; Flags: runhidden; StatusMsg: "Configuring Hardware Capture Filter Driver (Zero-Reboot)..."

; Native Firewall Rules Configuration (Zero PowerShell Dependency)
Filename: "{sys}\netsh.exe"; Parameters: "advfirewall firewall delete rule name=""USB Master Pro Daemon"""; Flags: runhidden
Filename: "{sys}\netsh.exe"; Parameters: "advfirewall firewall delete rule name=""USB Master Pro"""; Flags: runhidden
Filename: "{sys}\netsh.exe"; Parameters: "advfirewall firewall add rule name=""USB Master Pro Daemon"" dir=in action=allow program=""{app}\bin\USBMasterDaemon.exe"" enable=yes"; Flags: runhidden; StatusMsg: "Configuring Windows Firewall for USB Master Pro Daemon..."
Filename: "{sys}\netsh.exe"; Parameters: "advfirewall firewall add rule name=""USB Master Pro Daemon"" dir=out action=allow program=""{app}\bin\USBMasterDaemon.exe"" enable=yes"; Flags: runhidden
Filename: "{sys}\netsh.exe"; Parameters: "advfirewall firewall add rule name=""USB Master Pro"" dir=in action=allow program=""{app}\bin\{#MyAppExeName}"" enable=yes"; Flags: runhidden; StatusMsg: "Configuring Windows Firewall for USB Master Pro..."
Filename: "{sys}\netsh.exe"; Parameters: "advfirewall firewall add rule name=""USB Master Pro"" dir=out action=allow program=""{app}\bin\{#MyAppExeName}"" enable=yes"; Flags: runhidden

; Launch application after setup
Filename: "{app}\bin\{#MyAppExeName}"; Description: "{cm:LaunchProgram,{#StringChange(MyAppName, '&', '&&')}}"; Flags: nowait postinstall skipifsilent

[UninstallRun]
; Graceful process teardown to prevent locked file reboots
Filename: "{sys}\taskkill.exe"; Parameters: "/F /IM USBMasterDaemon.exe /IM {#MyAppExeName}"; Flags: runhidden

; Dynamic PnP Device Node Detachment & Driver Unloading (Zero-Reboot Invariant)
Filename: "{sys}\pnputil.exe"; Parameters: "/remove-device ""root\anyusb_vhci"""; Flags: runhidden; StatusMsg: "Dynamically detaching Virtual USB Device..."
Filename: "{sys}\pnputil.exe"; Parameters: "/delete-driver anyusb_vhci.inf /uninstall /force"; Flags: runhidden; StatusMsg: "Dynamically unloading Virtual USB Drivers..."
Filename: "{sys}\pnputil.exe"; Parameters: "/delete-driver anyusb_filter.inf /uninstall /force"; Flags: runhidden; StatusMsg: "Dynamically unloading Capture Filter Driver..."

; Clean up Windows Firewall rules
Filename: "{sys}\netsh.exe"; Parameters: "advfirewall firewall delete rule name=""USB Master Pro Daemon"""; Flags: runhidden; StatusMsg: "Cleaning up Windows Firewall rules..."
Filename: "{sys}\netsh.exe"; Parameters: "advfirewall firewall delete rule name=""USB Master Pro"""; Flags: runhidden
