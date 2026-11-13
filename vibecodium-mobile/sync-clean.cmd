@echo off
setlocal
cd /d "%~dp0"

powershell -NoProfile -ExecutionPolicy Bypass -File "%~dp0sync-clean.ps1" %*

if %ERRORLEVEL% NEQ 0 (
    echo.
    echo ===================================================
    echo   SYNC FAILED (Code %ERRORLEVEL%)
    echo ===================================================
)

endlocal
