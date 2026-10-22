param(
    [switch]$Install,
    [switch]$Release
)

$ErrorActionPreference = "Stop"

$sdk = "$env:LOCALAPPDATA\Android\Sdk"
$ndk = "$sdk\ndk\27.0.12077973"
$jbr = "C:\Program Files\Android\Android Studio\jbr"

$env:ANDROID_HOME = $sdk
$env:ANDROID_NDK_HOME = $ndk
$env:ANDROID_NDK_ROOT = $ndk
if (Test-Path $jbr) {
    $env:JAVA_HOME = $jbr
}
$env:PATH = "$env:USERPROFILE\.cargo\bin;$env:USERPROFILE\scoop\apps\mingw\current\bin;$sdk\platform-tools;$env:JAVA_HOME\bin;$env:PATH"

$jniLibs = Join-Path $PSScriptRoot "android\app\src\main\jniLibs"
$cargoProfile = if ($Release) { "--release" } else { "" }

Write-Host "==> Building Rust native libraries (arm64-v8a, x86_64) via cargo-ndk..."
if ($Release) {
    cargo ndk -t arm64-v8a -t x86_64 -o $jniLibs build --lib --release
} else {
    cargo ndk -t arm64-v8a -t x86_64 -o $jniLibs build --lib
}

Push-Location (Join-Path $PSScriptRoot "android")
try {
    $gradlew = Join-Path (Get-Location) "gradlew.bat"
    if (-not (Test-Path $gradlew)) {
        $cachedGradle = Get-ChildItem "$env:USERPROFILE\.gradle\wrapper\dists\gradle-8.12-all" -Recurse -Filter "gradle.bat" |
            Select-Object -First 1 -ExpandProperty FullName
        & $cachedGradle wrapper
    }
    $task = if ($Release) { "assembleRelease" } else { "assembleDebug" }
    Write-Host "==> Running Gradle $task..."
    & .\gradlew.bat $task
} finally {
    Pop-Location
}

$apk = Join-Path $PSScriptRoot "android\app\build\outputs\apk\debug\app-debug.apk"
if (Test-Path $apk) {
    Write-Host "==> APK built at: $apk"
    if ($Install) {
        Write-Host "==> Installing and launching on connected device/emulator..."
        adb install -r $apk
        adb shell am start -n com.example.androidapp/.MainActivity
    }
}
