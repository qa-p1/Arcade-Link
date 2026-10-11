!define ARCADE_ID "arcade.ci-nsis"
!define ARCADE_NAME "Arcade Link CI NSIS"
!ifndef ARCADE_VERSION
!define ARCADE_VERSION "0.3.0"
!endif
!ifndef ARCADE_SETUP_FILENAME
!define ARCADE_SETUP_FILENAME "sample-nsis.exe"
!endif
!define ARCADE_EXE "sample.exe"
!define ARCADE_CLI "arcade-ci-nsis"
!include "..\arcade.nsh"
!insertmacro ArcadeSetup
OutFile "${ARCADE_SETUP_FILENAME}"
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
