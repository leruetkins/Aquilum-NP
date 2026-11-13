@echo off
setlocal
cd /d "%~dp0"

echo ===================================================
echo   Building Vibecodium Mobile Release APK
echo ===================================================
echo.

call gradlew.bat assembleRelease %*

if %ERRORLEVEL% EQU 0 (
    echo.
    echo ===================================================
    echo   BUILD SUCCESSFUL!
    echo   APK: app\build\outputs\apk\release\app-release.apk
    echo ===================================================
) else (
    echo.
    echo ===================================================
    echo   BUILD FAILED (Code %ERRORLEVEL%)
    echo ===================================================
)

endlocal
