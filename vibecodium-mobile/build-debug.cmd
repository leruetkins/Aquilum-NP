@echo off
setlocal
cd /d "%~dp0"

echo ===================================================
echo   Building Vibecodium Mobile Debug APK
echo ===================================================
echo.

call gradlew.bat assembleDebug

if %ERRORLEVEL% EQU 0 (
    echo.
    echo ===================================================
    echo   BUILD SUCCESSFUL!
    echo   APK: app\build\outputs\apk\debug\app-debug.apk
    echo ===================================================
) else (
    echo.
    echo ===================================================
    echo   BUILD FAILED (Code %ERRORLEVEL%)
    echo ===================================================
)

endlocal
