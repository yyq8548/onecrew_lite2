; Per-user desktop install of the verified, embedded-UI onecrew_lite executable.
; The uninstaller removes only installed files, never local business data.
Unicode true
!include "MUI2.nsh"
!include "LogicLib.nsh"
!include "x64.nsh"
!ifndef OneCrewVersion
  !error "OneCrewVersion is required"
!endif
!ifndef OneCrewExe
  !error "OneCrewExe is required"
!endif
!ifndef OneCrewOutput
  !error "OneCrewOutput is required"
!endif

Name "onecrew_lite"
OutFile "${OneCrewOutput}\onecrew_lite-Setup-${OneCrewVersion}-windows-x64.exe"
InstallDir "$LOCALAPPDATA\onecrew_lite\bin"
RequestExecutionLevel user
SetCompressor /SOLID zlib
ShowInstDetails nevershow
ShowUninstDetails nevershow

!insertmacro MUI_PAGE_WELCOME
!insertmacro MUI_PAGE_INSTFILES
!define MUI_FINISHPAGE_RUN "$INSTDIR\onecrew_lite.exe"
!define MUI_FINISHPAGE_RUN_PARAMETERS "web"
!insertmacro MUI_PAGE_FINISH
!insertmacro MUI_UNPAGE_CONFIRM
!insertmacro MUI_UNPAGE_INSTFILES
!insertmacro MUI_LANGUAGE "English"
!insertmacro MUI_LANGUAGE "SimpChinese"

Function .onInit
  ${IfNot} ${RunningX64}
    MessageBox MB_ICONSTOP "onecrew_lite requires 64-bit Windows."
    Abort
  ${EndIf}
FunctionEnd

Section "Install"
  SetShellVarContext current
  SetOutPath "$INSTDIR"
  File "/oname=onecrew_lite.exe" "${OneCrewExe}"
  CreateDirectory "$SMPROGRAMS\onecrew_lite"
  CreateShortcut "$SMPROGRAMS\onecrew_lite\onecrew_lite.lnk" "$INSTDIR\onecrew_lite.exe" "web" "$INSTDIR\onecrew_lite.exe" 0 SW_SHOWMINIMIZED
  CreateShortcut "$DESKTOP\onecrew_lite.lnk" "$INSTDIR\onecrew_lite.exe" "web" "$INSTDIR\onecrew_lite.exe" 0 SW_SHOWMINIMIZED
  WriteUninstaller "$INSTDIR\Uninstall.exe"
  WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\onecrew_lite" "DisplayName" "onecrew_lite"
  WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\onecrew_lite" "DisplayVersion" "${OneCrewVersion}"
  WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\onecrew_lite" "Publisher" "onecrew_lite"
  WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\onecrew_lite" "InstallLocation" "$INSTDIR"
  WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\onecrew_lite" "UninstallString" "$\"$INSTDIR\Uninstall.exe$\""
SectionEnd

Section "Uninstall"
  SetShellVarContext current
  Delete "$DESKTOP\onecrew_lite.lnk"
  Delete "$SMPROGRAMS\onecrew_lite\onecrew_lite.lnk"
  RMDir "$SMPROGRAMS\onecrew_lite"
  Delete "$INSTDIR\onecrew_lite.exe"
  Delete "$INSTDIR\Uninstall.exe"
  RMDir "$INSTDIR"
  DeleteRegKey HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\onecrew_lite"
SectionEnd
