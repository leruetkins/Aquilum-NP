# Build script for Vibecodium Mobile (Debug APK)
param(
    [switch]$RebuildCore
)

$ErrorActionPreference = "Stop"
$ScriptDir = Split-Path -Parent $MyInvocation.MyCommand.Definition
Set-Location $ScriptDir

if ($RebuildCore) {
    Write-Host "Building Rust core..." -ForegroundColor Cyan
    Set-Location "$ScriptDir\core"
    cargo build --release --target aarch64-linux-android
    cargo build --release --target x86_64-linux-android

    New-Item -ItemType Directory -Force -Path "$ScriptDir\app\src\main\jniLibs\arm64-v8a" | Out-Null
    New-Item -ItemType Directory -Force -Path "$ScriptDir\app\src\main\jniLibs\x86_64" | Out-Null

    Copy-Item "$ScriptDir\core\target\aarch64-linux-android\release\libvibecodium_core.so" "$ScriptDir\app\src\main\jniLibs\arm64-v8a\" -Force
    Copy-Item "$ScriptDir\core\target\x86_64-linux-android\release\libvibecodium_core.so" "$ScriptDir\app\src\main\jniLibs\x86_64\" -Force
    Set-Location $ScriptDir
    Write-Host "Rust core copied to jniLibs." -ForegroundColor Green
}

Write-Host "Assembling Debug APK..." -ForegroundColor Cyan
& ".\gradlew.bat" assembleDebug

if ($LASTEXITCODE -eq 0) {
    $apkPath = "$ScriptDir\app\build\outputs\apk\debug\app-debug.apk"
    Write-Host "`nBUILD SUCCESSFUL!" -ForegroundColor Green
    Write-Host "APK: $apkPath" -ForegroundColor Yellow
} else {
    Write-Host "`nBUILD FAILED with exit code $LASTEXITCODE" -ForegroundColor Red
    exit $LASTEXITCODE
}
