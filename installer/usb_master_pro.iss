; ==============================================================================
; USB Master Pro — Production Inno Setup Installer Script
; Owned & Developed by BhartX Ecosystems Pvt Ltd (https://bhartx.in)
; Dynamic Driver Installation (reboot may be required if system or locked files require it) | Clean Control Panel Registration | x64 Native
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
RestartIfNeededByRun=yes
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

; Dynamic PnP Device Node Detachment & Driver Unloading
Filename: "{sys}\pnputil.exe"; Parameters: "/remove-device ""root\anyusb_vhci"""; Flags: runhidden; StatusMsg: "Dynamically detaching Virtual USB Device..."
Filename: "{sys}\pnputil.exe"; Parameters: "/delete-driver anyusb_vhci.inf /uninstall /force"; Flags: runhidden; StatusMsg: "Dynamically unloading Virtual USB Drivers..."
Filename: "{sys}\pnputil.exe"; Parameters: "/delete-driver anyusb_filter.inf /uninstall /force"; Flags: runhidden; StatusMsg: "Dynamically unloading Capture Filter Driver..."

; Clean up Windows Firewall rules
Filename: "{sys}\netsh.exe"; Parameters: "advfirewall firewall delete rule name=""USB Master Pro Daemon"""; Flags: runhidden; StatusMsg: "Cleaning up Windows Firewall rules..."
Filename: "{sys}\netsh.exe"; Parameters: "advfirewall firewall delete rule name=""USB Master Pro"""; Flags: runhidden

[Code]
var
  RestartRequired: Boolean;

// Comprehensive pre-flight validation checking all required driver files exist
procedure ValidateDriverFilesInstalled();
var
  AppDir: String;
  MissingList: String;
begin
  AppDir := ExpandConstant('{app}');
  MissingList := '';

  if not FileExists(AppDir + '\drivers\vhci\anyusb_vhci.inf') then
    MissingList := MissingList + '  - ' + AppDir + '\drivers\vhci\anyusb_vhci.inf' + #13#10;
  if not FileExists(AppDir + '\drivers\vhci\anyusb_vhci.sys') then
    MissingList := MissingList + '  - ' + AppDir + '\drivers\vhci\anyusb_vhci.sys' + #13#10;
  if not FileExists(AppDir + '\drivers\vhci\anyusb_vhci.cat') then
    MissingList := MissingList + '  - ' + AppDir + '\drivers\vhci\anyusb_vhci.cat' + #13#10;
  if not FileExists(AppDir + '\drivers\filter\anyusb_filter.inf') then
    MissingList := MissingList + '  - ' + AppDir + '\drivers\filter\anyusb_filter.inf' + #13#10;
  if not FileExists(AppDir + '\drivers\filter\anyusb_filter.sys') then
    MissingList := MissingList + '  - ' + AppDir + '\drivers\filter\anyusb_filter.sys' + #13#10;
  if not FileExists(AppDir + '\drivers\filter\anyusb_filter.cat') then
    MissingList := MissingList + '  - ' + AppDir + '\drivers\filter\anyusb_filter.cat' + #13#10;

  if MissingList <> '' then
  begin
    MsgBox('CRITICAL ERROR: Required driver package files are missing:' + #13#10#13#10 +
           MissingList + #13#10 +
           'Dynamic driver installation cannot proceed. Setup will now abort.',
           mbCriticalError, MB_OK);
    Abort();
  end;
end;

// Install driver package via pnputil and handle exit codes
function InstallDriverPackage(const InfPath: String): Boolean;
var
  PnpUtilPath: String;
  Params: String;
  ResultCode: Integer;
begin
  Result := False;
  PnpUtilPath := ExpandConstant('{sys}\pnputil.exe');
  Params := '/add-driver "' + InfPath + '" /install';

  if not Exec(PnpUtilPath, Params, '', SW_HIDE, ewWaitUntilTerminated, ResultCode) then
  begin
    MsgBox(Format('Driver installation failed: unable to execute pnputil.exe for %s', [InfPath]), mbCriticalError, MB_OK);
    Abort();
    Exit;
  end;

  if ResultCode = 0 then
  begin
    // Exit code 0: Success without reboot
    Result := True;
  end
  else if ResultCode = 3010 then
  begin
    // Exit code 3010 (ERROR_SUCCESS_REBOOT_REQUIRED): Success, but system restart required
    RestartRequired := True;
    Result := True;
  end
  else
  begin
    // Any other code: Show visible error modal: Driver installation failed: pnputil returned exit code %d and abort setup
    MsgBox(Format('Driver installation failed: pnputil returned exit code %d', [ResultCode]), mbCriticalError, MB_OK);
    Abort();
    Exit;
  end;
end;

function NeedRestart(): Boolean;
begin
  Result := RestartRequired;
end;

procedure CurStepChanged(CurStep: TSetupStep);
var
  AppDir: String;
begin
  if CurStep = ssPostInstall then
  begin
    ValidateDriverFilesInstalled();
    AppDir := ExpandConstant('{app}');
    InstallDriverPackage(AppDir + '\drivers\vhci\anyusb_vhci.inf');
    InstallDriverPackage(AppDir + '\drivers\filter\anyusb_filter.inf');
  end;
end;
