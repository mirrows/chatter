<#
.SYNOPSIS
  Builds a signed (debug keystore) arm64 release APK for Chatters.

.DESCRIPTION
  Reproduces the M9 Android packaging flow on Windows:
    1. (optional) npm run build  -> dist/
    2. (optional) cargo build --release --target aarch64-linux-android
    3. copy libchatters_lib.so into gen/android/.../jniLibs/arm64-v8a
    4. gradle assembleArm64Release  (rustBuild task excluded: the .so exists)
    5. zipalign + apksigner sign with the local debug keystore

  Output: src-tauri/target/android/Chatters-<version>-arm64-debugsigned.apk

.PARAMETER SkipFrontendBuild
  Reuse the existing dist/ folder instead of running npm run build.

.PARAMETER SkipRustBuild
  Reuse src-tauri/target/aarch64-linux-android/release/libchatters_lib.so.

.NOTES
  Requires JAVA_HOME (JDK 17) and ANDROID_HOME (build-tools 36.0.0 + NDK 27).
  The debug keystore is for sideload testing only and must not be used for
  store distribution (see requirements/01-project-overview.md, Q13).
#>
param(
    [switch]$SkipFrontendBuild,
    [switch]$SkipRustBuild,
    [string]$GradleExe = 'D:\tools\gradle\gradle-9.6.1\bin\gradle.bat',
    [string]$BuildToolsVersion = '36.0.0',
    [string]$RustTarget = 'aarch64-linux-android',
    [string]$AndroidAbi = 'arm64-v8a',
    [string]$GradleVariant = 'Arm64',
    [string]$ApkAbi = 'arm64',
    [int]$MinSdk = 24,
    [string]$DebugKeystore = '',
    [string]$DebugKeyAlias = 'androiddebugkey',
    [string]$DebugKeyStorePass = 'android',
    [string]$DebugKeyPass = 'android',
    [string]$ReleaseKeystore = '',
    [string]$ReleaseKeyAlias = 'chatters',
    [string]$ReleaseKeyStorePass = '',
    [string]$ReleaseKeyPass = ''
)

$ErrorActionPreference = 'Stop'

$repoRoot = Split-Path -Parent $PSScriptRoot
$srcTauri = Join-Path $repoRoot 'src-tauri'
$androidProject = Join-Path $srcTauri 'gen\android'
$jniLibDir = Join-Path $androidProject "app\src\main\jniLibs\$AndroidAbi"
$rustSo = Join-Path $srcTauri "target\$RustTarget\release\libchatters_lib.so"
$unsignedApk = Join-Path $androidProject "app\build\outputs\apk\$ApkAbi\release\app-$ApkAbi-release-unsigned.apk"
$gradleSignedApk = Join-Path $androidProject "app\build\outputs\apk\$ApkAbi\release\app-$ApkAbi-release.apk"
$outputDir = Join-Path $srcTauri 'target\android'

# release keystore 优先取参数，其次取环境变量（与 Gradle 的 CHATTERS_* 约定一致）。
if ([string]::IsNullOrWhiteSpace($ReleaseKeystore) -and $env:CHATTERS_KEYSTORE) {
    $script:ReleaseKeystore = $env:CHATTERS_KEYSTORE
}
if ([string]::IsNullOrWhiteSpace($ReleaseKeyStorePass) -and $env:CHATTERS_KEYSTORE_PASSWORD) {
    $script:ReleaseKeyStorePass = $env:CHATTERS_KEYSTORE_PASSWORD
}
if ($ReleaseKeyAlias -eq 'chatters' -and $env:CHATTERS_KEY_ALIAS) {
    $script:ReleaseKeyAlias = $env:CHATTERS_KEY_ALIAS
}
if ([string]::IsNullOrWhiteSpace($ReleaseKeyPass) -and $env:CHATTERS_KEY_PASSWORD) {
    $script:ReleaseKeyPass = $env:CHATTERS_KEY_PASSWORD
}
if (-not [string]::IsNullOrWhiteSpace($ReleaseKeystore) -and -not (Test-Path $ReleaseKeystore)) {
    throw "release keystore not found: $ReleaseKeystore"
}

function Assert-Environment {
    if (-not $env:JAVA_HOME) { throw 'JAVA_HOME is not set (JDK 17 is required by AGP 8.x)' }
    if (-not $env:ANDROID_HOME) { throw 'ANDROID_HOME is not set' }
    if (-not (Test-Path $GradleExe)) { throw "Gradle not found: $GradleExe" }

    $buildTools = Join-Path $env:ANDROID_HOME "build-tools\$BuildToolsVersion"
    if (-not (Test-Path $buildTools)) { throw "build-tools not found: $buildTools" }
    foreach ($exe in @('zipalign.exe', 'apksigner.bat', 'aapt2.exe')) {
        $path = Join-Path $buildTools $exe
        if (-not (Test-Path $path)) { throw "missing $exe in $buildTools" }
    }
    return $buildTools
}

$buildTools = Assert-Environment

# [0/5] Sync launcher icons.
# `tauri icon` writes src-tauri/icons/android/**; the Gradle project keeps its own
# copy under gen/android/app/src/main/res, so mirror it here. Without this step the
# APK ships the default Tauri launcher icon.
$iconSource = Join-Path $srcTauri 'icons\android'
$resDir = Join-Path $androidProject 'app\src\main\res'
if (-not (Test-Path $iconSource)) { throw "android icons not found: $iconSource" }
Write-Host '[0/5] sync android launcher icons'
Copy-Item (Join-Path $iconSource '*') $resDir -Recurse -Force
$sourceIcon = Join-Path $iconSource 'mipmap-xxxhdpi\ic_launcher.png'
$targetIcon = Join-Path $resDir 'mipmap-xxxhdpi\ic_launcher.png'
if ((Get-FileHash $sourceIcon -Algorithm SHA256).Hash -ne (Get-FileHash $targetIcon -Algorithm SHA256).Hash) {
    throw 'launcher icon sync failed (sha256 mismatch)'
}
Write-Host '      launcher icon synced'

if (-not $SkipFrontendBuild) {
    Write-Host '[1/5] npm run build'
    Push-Location $repoRoot
    try {
        & npm run build
        if ($LASTEXITCODE -ne 0) { throw "npm run build failed ($LASTEXITCODE)" }
    } finally { Pop-Location }
}

if (-not $SkipRustBuild) {
    Write-Host "[2/5] cargo build --release --target $RustTarget"
    $env:TAURI_ENV_DEBUG = 'false'

    # Cross-compiling needs the NDK clang wrapper as linker. Inject it through an
    # environment variable so no machine-specific absolute path is committed.
    $clangPrefix = switch ($RustTarget) {
        'aarch64-linux-android' { 'aarch64-linux-android' }
        'armv7-linux-androideabi' { 'armv7a-linux-androideabi' }
        'i686-linux-android' { 'i686-linux-android' }
        'x86_64-linux-android' { 'x86_64-linux-android' }
        default { throw "unsupported Rust target: $RustTarget" }
    }
    $ndkRoot = if ($env:NDK_HOME) { $env:NDK_HOME } else { Join-Path $env:ANDROID_HOME 'ndk\27.0.12077973' }
    $linker = Join-Path $ndkRoot "toolchains\llvm\prebuilt\windows-x86_64\bin\$clangPrefix$MinSdk-clang.cmd"
    if (-not (Test-Path $linker)) { throw "NDK clang not found: $linker (check NDK_HOME or pass -MinSdk)" }
    $linkerVar = 'CARGO_TARGET_' + ($RustTarget -replace '[^A-Za-z0-9]', '_').ToUpper() + '_LINKER'
    Set-Item -Path "env:$linkerVar" -Value $linker
    Write-Host "      linker: $linker"

    Push-Location $srcTauri
    try {
        & cargo build --release --target $RustTarget --features tauri/custom-protocol
        if ($LASTEXITCODE -ne 0) { throw "cargo build failed ($LASTEXITCODE)" }
    } finally { Pop-Location }
}

if (-not (Test-Path $rustSo)) { throw "Rust library not found: $rustSo" }

Write-Host "[3/5] copy $AndroidAbi/libchatters_lib.so into jniLibs"
New-Item -ItemType Directory -Force $jniLibDir | Out-Null
Copy-Item $rustSo (Join-Path $jniLibDir 'libchatters_lib.so') -Force

Write-Host "[4/5] gradle assemble${GradleVariant}Release (rustBuild excluded)"
Push-Location $androidProject
try {
    & $GradleExe "assemble${GradleVariant}Release" '-x' "rustBuild${GradleVariant}Release" '--console=plain'
    if ($LASTEXITCODE -ne 0) { throw "gradle assemble failed ($LASTEXITCODE)" }
} finally { Pop-Location }

$versionName = '1.0.0'
$tauriProps = Join-Path $androidProject 'app\tauri.properties'
if (Test-Path $tauriProps) {
    $match = Select-String -Path $tauriProps -Pattern '^tauri\.android\.versionName=(.+)$'
    if ($match) { $versionName = $match.Matches[0].Groups[1].Value.Trim() }
}

Write-Host '[5/5] 收集 release APK'
New-Item -ItemType Directory -Force $outputDir | Out-Null

$useReleaseKeystore = -not [string]::IsNullOrWhiteSpace($ReleaseKeystore) -and (Test-Path $ReleaseKeystore)
if (Test-Path $gradleSignedApk) {
    # Gradle 已用 release keystore 签名（CHATTERS_* 环境变量）→ 直接采用，避免二次签名。
    $signedApk = Join-Path $outputDir "Chatters-$versionName-$ApkAbi-release.apk"
    Copy-Item $gradleSignedApk $signedApk -Force
    Write-Host '      gradle already signed with the release keystore'
} else {
    if (-not (Test-Path $unsignedApk)) { throw "unsigned APK not found: $unsignedApk" }
    $alignedApk = Join-Path $outputDir "Chatters-$versionName-$ApkAbi-aligned.apk"
    $signedApk = if ($useReleaseKeystore) {
        Join-Path $outputDir "Chatters-$versionName-$ApkAbi-release.apk"
    } else {
        Join-Path $outputDir "Chatters-$versionName-$ApkAbi-debugsigned.apk"
    }

    & (Join-Path $buildTools 'zipalign.exe') -p -f 4 $unsignedApk $alignedApk
    if ($LASTEXITCODE -ne 0) { throw "zipalign failed ($LASTEXITCODE)" }

    if ($useReleaseKeystore) {
        Write-Host '      signing with the release keystore'
        & (Join-Path $buildTools 'apksigner.bat') sign `
            --ks $ReleaseKeystore --ks-key-alias $ReleaseKeyAlias `
            --ks-pass "pass:$ReleaseKeyStorePass" --key-pass "pass:$ReleaseKeyPass" `
            --out $signedApk $alignedApk
    } else {
        if ([string]::IsNullOrWhiteSpace($DebugKeystore)) {
            $script:DebugKeystore = Join-Path $env:USERPROFILE '.android\debug.keystore'
        }
        if (-not (Test-Path $DebugKeystore)) {
            throw "no release keystore configured and debug keystore not found: $DebugKeystore (run one Gradle debug build first)"
        }
        Write-Host '      signing with the debug keystore (sideload only)'
        & (Join-Path $buildTools 'apksigner.bat') sign `
            --ks $DebugKeystore --ks-key-alias $DebugKeyAlias `
            --ks-pass "pass:$DebugKeyStorePass" --key-pass "pass:$DebugKeyPass" `
            --out $signedApk $alignedApk
    }
    if ($LASTEXITCODE -ne 0) { throw "apksigner sign failed ($LASTEXITCODE)" }
}

& (Join-Path $buildTools 'apksigner.bat') verify --verbose --print-certs $signedApk
if ($LASTEXITCODE -ne 0) { throw "apksigner verify failed ($LASTEXITCODE)" }

Add-Type -AssemblyName System.IO.Compression.FileSystem
$zip = [System.IO.Compression.ZipFile]::OpenRead($signedApk)
try {
    $nativeLibs = @($zip.Entries | Where-Object { $_.FullName -like 'lib/*' })
    if ($nativeLibs.Count -eq 0) { throw "no native libraries packaged in $signedApk" }
    foreach ($entry in $nativeLibs) {
        Write-Host ("  {0}  {1:N2} MB" -f $entry.FullName, ($entry.Length / 1MB))
    }
} finally { $zip.Dispose() }

$hash = (Get-FileHash $signedApk -Algorithm SHA256).Hash
Write-Host ''
Write-Host "APK   : $signedApk"
Write-Host ("size  : {0:N2} MB" -f ((Get-Item $signedApk).Length / 1MB))
Write-Host "sha256: $hash"
if ($useReleaseKeystore -or (Test-Path $gradleSignedApk)) {
    Write-Host 'note  : signed with the release keystore (distribution ready).'
} else {
    Write-Host 'note  : signed with the Android debug keystore - sideload only; configure CHATTERS_KEYSTORE for a release build.'
}
