; Inno Setup 6 include. Define ARCADE_ID, ARCADE_NAME, ARCADE_VERSION,
; ARCADE_EXE (filename), ARCADE_CLI before including. See README.md.
#ifndef ARCADE_INSTALLER_ID
  #define ARCADE_INSTALLER_ID ARCADE_ID
#endif
#ifndef ARCADE_HOMEPAGE
  #define ARCADE_HOMEPAGE "https://github.com/qa-p1/Arcade-catalog"
#endif
#ifndef ARCADE_START_AT_LOGIN_DEFAULT
  #define ARCADE_START_AT_LOGIN_DEFAULT 0
#endif
#define ArcadeHelper AddBackslash(__DIR__) + "arcade-integration.ps1"

[Setup]
AppId={#ARCADE_INSTALLER_ID}
AppName={#ARCADE_NAME}
AppVersion={#ARCADE_VERSION}
AppPublisher=qa-p1
AppPublisherURL={#ARCADE_HOMEPAGE}
AppSupportURL={#ARCADE_HOMEPAGE}
DefaultDirName={localappdata}\Programs\{#ARCADE_NAME}
DefaultGroupName={#ARCADE_NAME}
PrivilegesRequired=lowest
DisableProgramGroupPage=yes
UninstallDisplayIcon={app}\{#ARCADE_EXE}

[Tasks]
Name: "arcade_desktop"; Description: "Create a desktop shortcut"; Flags: unchecked
#if ARCADE_START_AT_LOGIN_DEFAULT
Name: "arcade_startup"; Description: "Start at login"
#else
Name: "arcade_startup"; Description: "Start at login"; Flags: unchecked
#endif

[Files]
Source: "{#ArcadeHelper}"; DestDir: "{app}"; Flags: ignoreversion

[Icons]
Name: "{userprograms}\{#ARCADE_NAME}\{#ARCADE_NAME}"; Filename: "{app}\{#ARCADE_EXE}"
Name: "{userdesktop}\{#ARCADE_NAME}"; Filename: "{app}\{#ARCADE_EXE}"; Tasks: arcade_desktop
Name: "{userstartup}\{#ARCADE_NAME}"; Filename: "{app}\{#ARCADE_EXE}"; Parameters: "--background"; Tasks: arcade_startup

[Code]
function ArcadeQuote(Value: String): String;
begin
  Result := '"' + Value + '"';
end;

procedure ArcadeIntegration(Installing: Boolean);
var
  Params, Autostart: String;
  ExitCode: Integer;
begin
  Params := '-NoProfile -NonInteractive -ExecutionPolicy RemoteSigned -File ' +
    ArcadeQuote(ExpandConstant('{app}\arcade-integration.ps1'));
  if Installing then begin
    Autostart := '-';
    if WizardIsTaskSelected('arcade_startup') then
      Autostart := ExpandConstant('{userstartup}\{#ARCADE_NAME}.lnk');
    Params := Params + ' -Action Install -Version ' + ArcadeQuote('{#ARCADE_VERSION}') +
      ' -Channel ' + ArcadeQuote(ExpandConstant('{param:ARCADECHANNEL|stable}')) +
      ' -ManagedBy ' + ArcadeQuote(ExpandConstant('{param:ARCADEMANAGEDBY|self}')) +
      ' -DesktopEntry ' + ArcadeQuote(ExpandConstant('{userprograms}\{#ARCADE_NAME}\{#ARCADE_NAME}.lnk')) +
      ' -Autostart ' + ArcadeQuote(Autostart) +
      ' -Uninstaller ' + ArcadeQuote(ExpandConstant('{uninstallexe}'));
  end else
    Params := Params + ' -Action Uninstall';
  Params := Params + ' -Id ' + ArcadeQuote('{#ARCADE_ID}') +
    ' -CliName ' + ArcadeQuote('{#ARCADE_CLI}') +
    ' -ExePath ' + ArcadeQuote(ExpandConstant('{app}\{#ARCADE_EXE}'));
  if not Exec(ExpandConstant('{sys}\WindowsPowerShell\v1.0\powershell.exe'), Params,
      '', SW_HIDE, ewWaitUntilTerminated, ExitCode) then
    RaiseException('Could not start Arcade installation integration.');
  if ExitCode <> 0 then
    RaiseException('Arcade installation integration failed (exit ' + IntToStr(ExitCode) + ').');
end;

// Apps with existing event handlers call these from their own handlers and
// define ARCADE_CUSTOM_EVENTS to prevent duplicate declarations.
#ifndef ARCADE_CUSTOM_EVENTS
procedure CurStepChanged(CurStep: TSetupStep);
begin
  if CurStep = ssPostInstall then ArcadeIntegration(True);
end;
procedure CurUninstallStepChanged(CurUninstallStep: TUninstallStep);
begin
  if CurUninstallStep = usUninstall then ArcadeIntegration(False);
end;
#endif
