@echo off
setlocal

echo Building TOAPIPROXY installer...
echo.

set "PATH=%PATH%;%USERPROFILE%\.cargo\bin"
powershell.exe -NoLogo -NoProfile -ExecutionPolicy Bypass -File "%~dp0scripts\build-app.ps1" -Bundles nsis -CopyInstaller
if errorlevel 1 exit /b %errorlevel%

echo.
echo Build finished. Installer is in build\
pause
