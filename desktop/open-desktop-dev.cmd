@echo off
setlocal

pushd "%~dp0" || (
  echo Failed to enter desktop directory: %~dp0
  exit /b 1
)

powershell -NoProfile -ExecutionPolicy Bypass -File "%CD%\scripts\open-desktop-dev.ps1" %*
set EXIT_CODE=%ERRORLEVEL%

popd
exit /b %EXIT_CODE%
