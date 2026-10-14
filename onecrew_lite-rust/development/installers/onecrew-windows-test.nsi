Unicode true
!include "MUI2.nsh"
!include "LogicLib.nsh"
!include "x64.nsh"
!include "${OneCrewOutput}\payload.nsh"
Name "onecrew_lite 未签名测试版"
OutFile "${OneCrewOutput}\onecrew_lite-Test-Setup-${OneCrewVersion}-windows-x64-unsigned.exe"
InstallDir "$LOCALAPPDATA\onecrew_lite\bin"
RequestExecutionLevel user
SetCompressor /SOLID zlib
!define MUI_WELCOMEPAGE_TEXT "这是 onecrew_lite 未签名测试版，包含工作台和动态表格，无需安装开发工具。请先安装并登录 Codex，准备 Chrome。首次入口会联网安装只读搜索组件并连接 Codex，之后需要重启 Codex。平台登录由你自己完成，不会自动开始搜索。$\r$\n$\r$\n已有测试版请先卸载程序再安装，业务数据保留。"
!insertmacro MUI_PAGE_WELCOME
!insertmacro MUI_PAGE_INSTFILES
!define MUI_FINISHPAGE_RUN "$INSTDIR\首次使用.cmd"
!insertmacro MUI_PAGE_FINISH
!insertmacro MUI_UNPAGE_CONFIRM
!insertmacro MUI_UNPAGE_INSTFILES
!insertmacro MUI_LANGUAGE "SimpChinese"
Function .onInit
  ${IfNot} ${RunningX64}
    MessageBox MB_ICONSTOP "需要 64 位 Windows。" /SD IDOK
    SetErrorLevel 2
    Abort
  ${EndIf}
FunctionEnd
Section "Install"
  SetShellVarContext current
  IfFileExists "$INSTDIR\onecrew_lite.exe" 0 +4
    MessageBox MB_ICONSTOP "请先卸载此目录中的旧测试版。卸载会保留业务数据。" /SD IDOK
    SetErrorLevel 3
    Abort
  !insertmacro InstallPayload
  WriteUninstaller "$INSTDIR\Uninstall.exe"
  CreateDirectory "$SMPROGRAMS\onecrew_lite Test"
  CreateShortcut "$SMPROGRAMS\onecrew_lite Test\onecrew_lite Test.lnk" "$INSTDIR\首次使用.cmd" "" "$INSTDIR\onecrew_lite.exe"
  CreateShortcut "$DESKTOP\onecrew_lite Test.lnk" "$INSTDIR\首次使用.cmd" "" "$INSTDIR\onecrew_lite.exe"
  CreateShortcut "$SMPROGRAMS\onecrew_lite Test\连接 Codex.lnk" "$INSTDIR\连接 Codex.cmd"
  CreateShortcut "$SMPROGRAMS\onecrew_lite Test\使用说明.lnk" "$INSTDIR\Windows测试版使用说明.txt"
  WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\onecrew_lite_Test" "DisplayName" "onecrew_lite 未签名测试版"
  WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\onecrew_lite_Test" "DisplayVersion" "${OneCrewVersion}"
  WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\onecrew_lite_Test" "UninstallString" '$\"$INSTDIR\Uninstall.exe$\"'
  WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\onecrew_lite_Test" "InstallLocation" "$INSTDIR"
SectionEnd
Section "Uninstall"
  SetShellVarContext current
  ClearErrors
  Delete "$INSTDIR\onecrew_lite_library\onecrew_lite.exe"
  IfErrors 0 +4
    MessageBox MB_ICONSTOP "动态表格仍在运行。请重启电脑后先卸载，再打开 onecrew_lite。数据会保留。" /SD IDOK
    SetErrorLevel 4
    Abort
  !insertmacro UninstallPayload
  Delete "$INSTDIR\.first-use.done"
  Delete "$DESKTOP\onecrew_lite Test.lnk"
  Delete "$SMPROGRAMS\onecrew_lite Test\onecrew_lite Test.lnk"
  Delete "$SMPROGRAMS\onecrew_lite Test\连接 Codex.lnk"
  Delete "$SMPROGRAMS\onecrew_lite Test\使用说明.lnk"
  RMDir "$SMPROGRAMS\onecrew_lite Test"
  Delete "$INSTDIR\Uninstall.exe"
  RMDir "$INSTDIR"
  DeleteRegKey HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\onecrew_lite_Test"
SectionEnd
