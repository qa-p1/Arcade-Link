!define ARCADE_ID "arcade.ci-nsis"
!define ARCADE_NAME "Arcade Link CI NSIS"
!define ARCADE_VERSION "0.3.0"
!define ARCADE_EXE "sample.exe"
!define ARCADE_CLI "arcade-ci-nsis"
!include "..\arcade.nsh"
!insertmacro ArcadeSetup
OutFile "sample-nsis.exe"
Page components
Page directory
Page instfiles
UninstPage instfiles
!insertmacro ArcadeTasks
Section "Application" ArcadeApplication
  SectionIn RO
  SetOutPath "$INSTDIR"
  File "sample.exe"
  !insertmacro ArcadeInstall
SectionEnd
Function .onInit
  !insertmacro ArcadeInitTasks
FunctionEnd
Section "Uninstall"
  !insertmacro ArcadeUninstall
  Delete "$INSTDIR\sample.exe"
  Delete "$INSTDIR\uninstall.exe"
  RMDir "$INSTDIR"
SectionEnd
