#define ARCADE_ID "arcade.ci-inno"
#define ARCADE_NAME "Arcade Link CI Inno"
#ifndef ARCADE_VERSION
#define ARCADE_VERSION "0.3.0"
#endif
#ifndef ARCADE_SETUP_FILENAME
#define ARCADE_SETUP_FILENAME "sample-inno"
#endif
#define ARCADE_EXE "sample.exe"
#define ARCADE_CLI "arcade-ci-inno"
#include "..\arcade.iss"
[Setup]
OutputDir=.
OutputBaseFilename={#ARCADE_SETUP_FILENAME}
[Files]
Source: "sample.exe"; DestDir: "{app}"; Flags: ignoreversion
