@echo off
chcp 65001 >nul
setlocal
cd /d "%~dp0"
if exist "%~dp0.first-use.done" goto launch
echo 首次准备搜索组件，需要联网，请稍等。
"%~dp0onecrew_lite.exe" opencli repair
if errorlevel 1 goto repair_failed
echo 正在连接 Codex。
"%~dp0onecrew_lite.exe" plugin install
if errorlevel 1 goto codex_failed
>"%~dp0.first-use.done" echo ready
echo.
echo 准备完成。请完全退出并重新打开 Codex。
echo 新建对话输入：打开 onecrew_lite
echo 之后双击桌面 onecrew_lite Test 即可打开工作台和表格。
pause
exit /b 0
:launch
"%~dp0onecrew_lite.exe" workspace
if errorlevel 1 goto launch_failed
exit /b 0
:repair_failed
echo 搜索组件准备失败。请检查网络，将上方报错截图发给提供安装包的人。
pause
exit /b 1
:codex_failed
echo Codex 连接失败。请先安装并登录 Codex，再双击桌面入口重试。
echo 若仍失败，请将上方报错截图发给提供安装包的人。
pause
exit /b 1
:launch_failed
echo 页面未能打开，请将上方报错截图发给提供安装包的人。
pause
exit /b 1
