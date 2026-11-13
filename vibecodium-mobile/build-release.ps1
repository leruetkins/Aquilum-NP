# Build script for Vibecodium Mobile (Release APK)
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

if (Test-Path "$ScriptDir\keystore.properties") {
    Write-Host "Using signing configuration from keystore.properties" -ForegroundColor DarkGray
} else {
    Write-Host "Warning: keystore.properties not found. Release APK might not be signed." -ForegroundColor Yellow
}

Write-Host "Assembling Release APK..." -ForegroundColor Cyan
& ".\gradlew.bat" assembleRelease

if ($LASTEXITCODE -eq 0) {
    $apkPath = "$ScriptDir\app\build\outputs\apk\release\app-release.apk"
    Write-Host "`n===================================================" -ForegroundColor Green
    Write-Host "  BUILD SUCCESSFUL!" -ForegroundColor Green
    Write-Host "  APK: $apkPath" -ForegroundColor Yellow
    Write-Host "===================================================" -ForegroundColor Green
} else {
    Write-Host "`nBUILD FAILED with exit code $LASTEXITCODE" -ForegroundColor Red
    exit $LASTEXITCODE
}
