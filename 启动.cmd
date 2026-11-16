@echo off
cd /d "%~dp0"
if exist "dist\LingDongIsland-0.2.3-windows-x64\*.exe" (
  for %%G in ("dist\LingDongIsland-0.2.3-windows-x64\*.exe") do start "" "%%~fG"
) else (
  powershell.exe -NoProfile -File "%~dp0scripts\dev.ps1" -Action Run
)