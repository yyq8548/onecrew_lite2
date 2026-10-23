@echo off
powershell.exe -NoProfile -ExecutionPolicy Bypass -File "%~dp0OneCrew.ps1" -Action Upgrade
set "onecrewExitCode=%errorlevel%"
if not "%onecrewExitCode%"=="0" echo Upgrade failed. Read the error above.
pause
exit /b %onecrewExitCode%
