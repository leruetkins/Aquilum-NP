# Sync script for Vibecodium Mobile.
#
# Two layouts are supported:
#
#   nested (default)  The clean mobile project is placed inside the desktop
#                     project: D:\Naben\Github\vibecodium\vibecodium-mobile.
#                     The mobile Rust core keeps using the desktop modules by
#                     path (`#[path]`), rewritten to point at
#                     `D:\Naben\Github\vibecodium\src`. Shared sources are not
#                     copied, so the two front ends cannot drift.
#
#   standalone        The clean project is self-contained: the desktop modules
#                     the core includes are copied into `core/src` and the
#                     `#[path]` attributes are removed.
#                     powershell ... -File .\sync-clean.ps1 -Standalone
#
# Always excluded (generated, machine-specific or secret):
#   build/, app/build/, .gradle/, .kotlin/, target/, core/target/,
#   screenshots/, keystore/, keystore.properties

param(
    [string]$Source = "D:\Naben\Github\rust\Aquilum-NP\vibecodium-mobile",
    [string]$Target = "",
    [string]$SharedSrc = "",
    [switch]$Standalone
)

$ErrorActionPreference = "Stop"

function Normalize-Path([string]$Path) {
    return [System.IO.Path]::GetFullPath($Path).TrimEnd('\')
}

function Get-RelativePath([string]$FromDir, [string]$ToPath) {
    $from = New-Object System.Uri(((Normalize-Path $FromDir) + '\'))
    $to = New-Object System.Uri((Normalize-Path $ToPath))
    $relative = $from.MakeRelativeUri($to).ToString()
    return [System.Uri]::UnescapeDataString($relative).Replace('/', '\')
}

$Source = Normalize-Path $Source
if (-not $Target) {
    $Target = if ($Standalone) {
        "D:\Naben\Github\vibecodium-mobile"
    } else {
        "D:\Naben\Github\vibecodium\vibecodium-mobile"
    }
}
if (-not $SharedSrc) {
    $SharedSrc = if ($Standalone) {
        "D:\Naben\Github\rust\Aquilum-NP\aquilum-gpui\src"
    } else {
        "D:\Naben\Github\vibecodium\src"
    }
}
$Target = Normalize-Path $Target
$SharedSrc = Normalize-Path $SharedSrc

if (-not (Test-Path -LiteralPath $Source)) { throw "Source not found: $Source" }
if (-not (Test-Path -LiteralPath $SharedSrc)) { throw "Shared sources not found: $SharedSrc" }
if ($Target -eq $Source -or $Target.StartsWith($Source + '\', [System.StringComparison]::OrdinalIgnoreCase)) {
    throw "Target must not be the source or a directory inside it: $Target"
}

New-Item -ItemType Directory -Force -Path $Target | Out-Null

function Invoke-Robocopy {
    param([string]$From, [string]$To, [string[]]$Extra = @())
    New-Item -ItemType Directory -Force -Path $To | Out-Null
    $arguments = @(
        $From, $To, '/MIR', '/NFL', '/NDL', '/NJH', '/NJS', '/NP', '/R:2', '/W:1'
    ) + $Extra
    & robocopy @arguments | Out-Null
    $code = $LASTEXITCODE
    # robocopy: 0-7 are success codes, 8+ are failures.
    if ($code -ge 8) { throw "robocopy failed with code $code ($From -> $To)" }
}

Write-Host "Copying app, gradle and scripts..." -ForegroundColor Cyan
Invoke-Robocopy "$Source\app" "$Target\app" @('/XD', "$Source\app\build")
Invoke-Robocopy "$Source\gradle" "$Target\gradle"

# build-core.sh is the documented way to rebuild the JNI libraries. The ADB
# scroll-jitter test is a development tool and stays in the working project.
New-Item -ItemType Directory -Force -Path "$Target\scripts" | Out-Null
Copy-Item -LiteralPath "$Source\scripts\build-core.sh" -Destination "$Target\scripts\build-core.sh" -Force
foreach ($name in @('test_scroll_jitter.py')) {
    $stale = Join-Path "$Target\scripts" $name
    if (Test-Path -LiteralPath $stale) {
        Remove-Item -LiteralPath $stale -Force
    }
}

Write-Host "Copying Rust core (without cargo target)..." -ForegroundColor Cyan
Invoke-Robocopy "$Source\core" "$Target\core" @('/XD', "$Source\core\target")

$rootFiles = @(
    '.gitignore',
    'README.md',
    'build.gradle.kts',
    'settings.gradle.kts',
    'gradle.properties',
    'gradlew',
    'gradlew.bat',
    'keystore.properties.example',
    'vibecodium-mobile.jpg'
)
foreach ($name in $rootFiles) {
    $from = Join-Path $Source $name
    if (Test-Path -LiteralPath $from) {
        Copy-Item -LiteralPath $from -Destination (Join-Path $Target $name) -Force
    }
}

# The sync script belongs to the working project only; never leave a copy in
# the generated clean directory (including copies from older runs). The local
# build helpers and the machine-specific SDK path are not part of the clean
# project either.
foreach ($name in @('sync-clean.ps1', 'sync-clean.cmd', 'build-debug.ps1', 'build-debug.cmd', 'local.properties')) {
    $stale = Join-Path $Target $name
    if (Test-Path -LiteralPath $stale) {
        Remove-Item -LiteralPath $stale -Force
    }
}

# The mobile core reuses desktop Rust modules via
# `#[path = "...aquilum-gpui/src/..."]`. The two layouts handle that differently.
$pathAttribute = [regex]'^\s*#\[path\s*=\s*"([^"]+)"\s*\]\s*$'
$sharedMarker = 'aquilum-gpui\src\'

function Materialize-SharedModules {
    param([string]$CoreSrc, [string]$DesktopSrc)

    $modLine = [regex]'^\s*(pub(?:\([^)]*\))?\s+)?mod\s+([A-Za-z_][A-Za-z0-9_]*)\s*;'
    $pending = New-Object 'System.Collections.Generic.Queue[string]'
    Get-ChildItem -LiteralPath $CoreSrc -Recurse -Filter *.rs -File |
        ForEach-Object { $pending.Enqueue($_.FullName) }

    $seen = @{}
    $materialized = 0
    while ($pending.Count -gt 0) {
        $file = $pending.Dequeue()
        if ($seen.ContainsKey($file)) { continue }
        $seen[$file] = $true

        $lines = [System.IO.File]::ReadAllLines($file)
        $result = New-Object 'System.Collections.Generic.List[string]'
        $rewrote = $false

        for ($i = 0; $i -lt $lines.Length; $i++) {
            $attribute = $pathAttribute.Match($lines[$i])
            if ($attribute.Success -and ($i + 1) -lt $lines.Length) {
                $moduleMatch = $modLine.Match($lines[$i + 1])
                if ($moduleMatch.Success) {
                    $relative = $attribute.Groups[1].Value.Replace('/', '\')
                    $module = $moduleMatch.Groups[2].Value
                    $fileDir = Split-Path -Parent $file
                    $markerIndex = $relative.IndexOf(
                        $sharedMarker,
                        [System.StringComparison]::OrdinalIgnoreCase
                    )

                    if ($markerIndex -ge 0) {
                        # Desktop module: copy it next to the declaring file and
                        # remove the attribute so `mod name;` resolves locally.
                        $sharedRelative = $relative.Substring($markerIndex + $sharedMarker.Length)
                        $from = Join-Path $DesktopSrc $sharedRelative
                        if (-not (Test-Path -LiteralPath $from)) {
                            throw "Shared module not found: $from (referenced by $file)"
                        }
                        # In Rust 2018, submodules of `markdown.rs` live in
                        # `markdown/`, while `mod.rs`/`lib.rs` use their own dir.
                        $declaringName = [System.IO.Path]::GetFileName($file)
                        $moduleDir = if ($declaringName -eq 'mod.rs' -or $declaringName -eq 'lib.rs') {
                            $fileDir
                        } else {
                            Join-Path $fileDir ([System.IO.Path]::GetFileNameWithoutExtension($file))
                        }
                        $to = Join-Path $moduleDir ($module + '.rs')
                        New-Item -ItemType Directory -Force -Path $moduleDir | Out-Null
                        Copy-Item -LiteralPath $from -Destination $to -Force
                        $pending.Enqueue($to)
                        $materialized += 1
                        $rewrote = $true
                        continue
                    }

                    # Relative include inside a shared file. The mirrored
                    # desktop path is <desktop>\<dir>\<path>.
                    $relativeToCore = $file.Substring($CoreSrc.Length).TrimStart('\')
                    $relativeDir = Split-Path -Parent $relativeToCore
                    if ($relativeDir) {
                        $from = Join-Path (Join-Path $DesktopSrc $relativeDir) $relative
                    } else {
                        $from = Join-Path $DesktopSrc $relative
                    }
                    if (Test-Path -LiteralPath $from) {
                        $to = if ($relativeDir) {
                            Join-Path (Join-Path $CoreSrc $relativeDir) $relative
                        } else {
                            Join-Path $CoreSrc $relative
                        }
                        New-Item -ItemType Directory -Force -Path (Split-Path -Parent $to) | Out-Null
                        Copy-Item -LiteralPath $from -Destination $to -Force
                        $pending.Enqueue($to)
                        $materialized += 1
                    }
                }
            }
            $result.Add($lines[$i])
        }

        if ($rewrote) {
            [System.IO.File]::WriteAllLines(
                $file,
                $result,
                (New-Object System.Text.UTF8Encoding($false))
            )
        }
    }
    return $materialized
}

function Rewire-SharedModules {
    param([string]$CoreSrc, [string]$DesktopSrc)

    $rewired = 0
    Get-ChildItem -LiteralPath $CoreSrc -Recurse -Filter *.rs -File | ForEach-Object {
        $file = $_.FullName
        $lines = [System.IO.File]::ReadAllLines($file)
        $changed = $false
        for ($i = 0; $i -lt $lines.Length; $i++) {
            $attribute = $pathAttribute.Match($lines[$i])
            if (-not $attribute.Success) { continue }
            $relative = $attribute.Groups[1].Value.Replace('/', '\')
            $markerIndex = $relative.IndexOf(
                $sharedMarker,
                [System.StringComparison]::OrdinalIgnoreCase
            )
            if ($markerIndex -lt 0) { continue }
            $sharedRelative = $relative.Substring($markerIndex + $sharedMarker.Length)
            $sharedFile = Join-Path $DesktopSrc $sharedRelative
            if (-not (Test-Path -LiteralPath $sharedFile)) {
                throw "Shared module not found: $sharedFile (referenced by $file)"
            }
            $newRelative = Get-RelativePath -FromDir (Split-Path -Parent $file) -ToPath $sharedFile
            $indent = $attribute.Value.Substring(0, $attribute.Value.Length - $attribute.Value.TrimStart().Length)
            $lines[$i] = $indent + '#[path = "' + $newRelative.Replace('\', '/') + '"]'
            $changed = $true
            $rewired += 1
        }
        if ($changed) {
            [System.IO.File]::WriteAllLines(
                $file,
                $lines,
                (New-Object System.Text.UTF8Encoding($false))
            )
        }
    }
    return $rewired
}

if ($Standalone) {
    Write-Host "Materializing shared desktop modules into core/src..." -ForegroundColor Cyan
    $count = Materialize-SharedModules -CoreSrc "$Target\core\src" -DesktopSrc $SharedSrc
    $mode = "standalone"
} else {
    Write-Host "Pointing core #[path] modules at $SharedSrc..." -ForegroundColor Cyan
    $count = Rewire-SharedModules -CoreSrc "$Target\core\src" -DesktopSrc $SharedSrc
    $mode = "nested"
}

# Every #[path] must resolve from the file that declares it.
$missing = @()
Get-ChildItem -LiteralPath "$Target\core\src" -Recurse -Filter *.rs -File | ForEach-Object {
    $file = $_.FullName
    $dir = Split-Path -Parent $file
    foreach ($line in [System.IO.File]::ReadAllLines($file)) {
        $attribute = $pathAttribute.Match($line)
        if (-not $attribute.Success) { continue }
        $resolved = Join-Path $dir $attribute.Groups[1].Value.Replace('/', '\')
        if (-not (Test-Path -LiteralPath $resolved)) {
            $missing += "$file -> $($attribute.Groups[1].Value)"
        }
    }
}
if ($missing) {
    throw "Unresolved #[path] includes:`n$($missing -join "`n")"
}

Write-Host ""
Write-Host "Clean copy is ready." -ForegroundColor Green
Write-Host "  Mode:        $mode"
Write-Host "  Source:      $Source"
Write-Host "  Target:      $Target"
Write-Host "  Shared src:  $SharedSrc"
Write-Host "  Shared .rs:  $count"
Write-Host ""
Write-Host "Not copied: build outputs, screenshots, keystore/ and keystore.properties." -ForegroundColor Yellow
Write-Host "Release signing stays disabled until you add your own keystore.properties." -ForegroundColor Yellow
Write-Host "local.properties is machine-specific: Android Studio creates it on open." -ForegroundColor Yellow
Write-Host "The sync script itself stays in the working project only." -ForegroundColor Yellow
if (-not $Standalone) {
    Write-Host "The mobile folder lives inside the desktop repo; add it to that repo's .gitignore or track it." -ForegroundColor Yellow
}
