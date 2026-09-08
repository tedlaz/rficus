

#define MyAppName "rficus"
; Passed in by the release workflow as /DMyAppVersion=X.Y.Z, from the git tag.
; A local build with no argument stamps 0.0.0 on purpose: a made-up number is
; better than a real one left over from an older release.
#ifndef MyAppVersion
  #define MyAppVersion "0.0.0"
#endif
#define MyAppPublisher "Ted Lazaros"
#define MyAppURL "https://github.com/tedlaz/rficus"
#define MyAppExeName "rficus.exe"

[Setup]
; NOTE: The value of AppId uniquely identifies this application. Do not use the same AppId value in installers for other applications.
; (To generate a new GUID, click Tools | Generate GUID inside the IDE.)
AppId={{8D9B34AD-FEB4-422D-B051-E280129190CB}
AppName={#MyAppName}
AppVersion={#MyAppVersion}
;AppVerName={#MyAppName} {#MyAppVersion}
AppPublisher={#MyAppPublisher}
AppPublisherURL={#MyAppURL}
AppSupportURL={#MyAppURL}
AppUpdatesURL={#MyAppURL}
DefaultDirName={%USERPROFILE}\{#MyAppName}
DisableProgramGroupPage=yes
; Per-user, and deliberately not elevated: rficus downloads yt-dlp and ffmpeg
; into {app} on first run, so it has to be able to write there while running as
; the user. An elevated install under Program Files could not.
PrivilegesRequired=lowest
OutputDir=.
OutputBaseFilename=setup_rficus.{#MyAppVersion}
SetupIconFile=.\ficus_setup.ico
UninstallDisplayIcon={app}\{#MyAppExeName}
Compression=lzma
SolidCompression=yes
WizardStyle=modern

[Languages]
Name: "english"; MessagesFile: "compiler:Default.isl"
Name: "greek"; MessagesFile: "Greek.isl"

[Tasks]
Name: "desktopicon"; Description: "{cm:CreateDesktopIcon}"; GroupDescription: "{cm:AdditionalIcons}"; Flags: unchecked

[Files]
; Just the app. yt-dlp, ffmpeg and ffprobe are fetched into {app} on first run,
; which also keeps them current instead of shipping whatever was bundled.
Source: ".\target\release\{#MyAppExeName}"; DestDir: "{app}"; Flags: ignoreversion

[Icons]
Name: "{autoprograms}\{#MyAppName}"; Filename: "{app}\{#MyAppExeName}"
Name: "{autodesktop}\{#MyAppName}"; Filename: "{app}\{#MyAppExeName}"; Tasks: desktopicon

[Run]
Filename: "{app}\{#MyAppExeName}"; Description: "{cm:LaunchProgram,{#StringChange(MyAppName, '&', '&&')}}"; Flags: nowait postinstall skipifsilent

[UninstallDelete]
Type: files; Name: "{app}\ficus.ini"
; rficus downloads these itself when they are missing, so uninstall must
; clear them too or ~145 MB is left behind.
Type: files; Name: "{app}\yt-dlp.exe"
Type: files; Name: "{app}\ffmpeg.exe"
Type: files; Name: "{app}\ffprobe.exe"
; the ffmpeg build is shared, so its libraries sit alongside the exes
Type: files; Name: "{app}\*.dll"
