#define ARCADE_ID "arcade.ci-inno"
#define ARCADE_NAME "Arcade Link CI Inno"
#define ARCADE_VERSION "0.3.0"
#define ARCADE_EXE "sample.exe"
#define ARCADE_CLI "arcade-ci-inno"
#include "..\arcade.iss"
[Setup]
OutputDir=.
OutputBaseFilename=sample-inno
[Files]
Source: "sample.exe"; DestDir: "{app}"; Flags: ignoreversion
