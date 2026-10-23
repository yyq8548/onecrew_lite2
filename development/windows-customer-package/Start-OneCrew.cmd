@echo off
powershell.exe -NoProfile -ExecutionPolicy Bypass -File "%~dp0OneCrew.ps1" -Action Start
set "onecrewExitCode=%errorlevel%"
if not "%onecrewExitCode%"=="0" echo Start failed. Read the error above.
pause
exit /b %onecrewExitCode%
