$ErrorActionPreference = 'Stop'

$repo = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
$packageTarget = Join-Path $repo 'target\package-build'
$dist = Join-Path $repo 'dist'
$stage = Join-Path $dist 'Voidspace-0.1.2-windows-x64'
$archive = Join-Path $dist 'Voidspace-0.1.2-windows-x64.zip'

function Remove-WithRetry([string]$Path, [switch]$Recurse) {
    if (-not (Test-Path -LiteralPath $Path)) { return }
    for ($attempt = 1; $attempt -le 6; $attempt++) {
        try {
            Remove-Item -LiteralPath $Path -Force -Recurse:$Recurse -ErrorAction Stop
            return
        }
        catch {
            if ($attempt -eq 6) { throw }
            Start-Sleep -Milliseconds (200 * $attempt)
        }
    }
}

function Get-Sha256([string]$Path) {
    $stream = [System.IO.File]::OpenRead($Path)
    try {
        $sha = [System.Security.Cryptography.SHA256]::Create()
        try {
            return ([System.BitConverter]::ToString($sha.ComputeHash($stream))).Replace('-', '').ToLowerInvariant()
        }
        finally { $sha.Dispose() }
    }
    finally { $stream.Dispose() }
}

Push-Location $repo
try {
    $env:CARGO_TARGET_DIR = $packageTarget
    & powershell -NoProfile -ExecutionPolicy Bypass -File (Join-Path $PSScriptRoot 'sync-font-assets.ps1') -Check
    if ($LASTEXITCODE -ne 0) { throw 'font asset verification failed' }
    & cargo fmt --all -- --check
    if ($LASTEXITCODE -ne 0) { throw 'cargo fmt failed' }
    & cargo clippy --workspace --all-targets -- -D warnings
    if ($LASTEXITCODE -ne 0) { throw 'cargo clippy failed' }
    & cargo test --workspace
    if ($LASTEXITCODE -ne 0) { throw 'cargo test failed' }
    & cargo build --workspace --release
    if ($LASTEXITCODE -ne 0) { throw 'release build failed' }
    & powershell -NoProfile -ExecutionPolicy Bypass -File (Join-Path $PSScriptRoot 'smoke.ps1')
    if ($LASTEXITCODE -ne 0) { throw 'smoke failed' }

    New-Item -ItemType Directory -Force $dist | Out-Null
    Remove-WithRetry -Path $stage -Recurse
    Remove-WithRetry -Path $archive
    New-Item -ItemType Directory -Force $stage | Out-Null
    Copy-Item -LiteralPath (Join-Path $packageTarget 'release\voidspace.exe') -Destination $stage
    Copy-Item -LiteralPath (Join-Path $packageTarget 'release\voidspace-elevated.exe') -Destination $stage
    Copy-Item -LiteralPath (Join-Path $repo 'crates\voidspace-app\assets\voidspace.ico') -Destination $stage
    Copy-Item -LiteralPath (Join-Path $repo 'README.md') -Destination $stage
    Copy-Item -LiteralPath (Join-Path $repo 'LICENSE') -Destination $stage

    $licenseMap = @(
        @{ Source='crates\voidspace-app\assets\licenses\fonts\unbounded\OFL.txt'; Destination='licenses\fonts\unbounded\OFL.txt' },
        @{ Source='crates\voidspace-app\assets\licenses\fonts\golostext\OFL.txt'; Destination='licenses\fonts\golostext\OFL.txt' },
        @{ Source='crates\voidspace-app\assets\licenses\fonts\jetbrainsmono\OFL.txt'; Destination='licenses\fonts\jetbrainsmono\OFL.txt' },
        @{ Source='crates\voidspace-app\assets\licenses\fonts\hack\Hack-Regular.txt'; Destination='licenses\fonts\hack\Hack-Regular.txt' },
        @{ Source='crates\voidspace-app\assets\licenses\fonts\ubuntu\UFL.txt'; Destination='licenses\fonts\ubuntu\UFL.txt' }
    )
    foreach ($entry in $licenseMap) {
        $from = Join-Path $repo $entry.Source
        $to = Join-Path $stage $entry.Destination
        New-Item -ItemType Directory -Force -Path (Split-Path -Parent $to) | Out-Null
        Copy-Item -LiteralPath $from -Destination $to
        if ((Get-Sha256 $from) -ne (Get-Sha256 $to)) { throw "Staged font notice mismatch: $($entry.Destination)" }
    }

    $fontNotice = @(
        'Voidspace third-party fonts',
        'Google Fonts revision: 6a003b5eb672dc8bf5bff5937cf5863f8b175445',
        'Unbounded variable TTF SHA-256: 323b511be380c8d474ef030686b71aedde501f8d9cd46da558b7c40454372c3f',
        'Golos Text variable TTF SHA-256: 17bb58fb69aec2dfb047a2ebf52534023e9b688c97a6b7ac795b0a72912c2063',
        'JetBrains Mono variable TTF SHA-256: 48715a42ec242c21e9f02692891e147d022299a52e48d5e413e1a942193ffeda',
        'Hack fallback TTF SHA-256: 15f55cc0c85a2988d2b4b3a8cdb5d77fdfbaf319e1bb5309d725db9818fb7125',
        'Ubuntu Light fallback TTF SHA-256: 80307b8da7649aa4ee4d484b232140e3ce1ec0ca093073d3c53c8f5a5ced7a70'
    )
    [System.IO.File]::WriteAllLines((Join-Path $stage 'THIRD-PARTY-FONTS.txt'), $fontNotice)

    $checksums = Get-ChildItem -LiteralPath $stage -Recurse -File | Sort-Object FullName | ForEach-Object {
        $relative = $_.FullName.Substring($stage.Length + 1).Replace('\', '/')
        "$(Get-Sha256 $_.FullName)  $relative"
    }
    [System.IO.File]::WriteAllLines((Join-Path $stage 'SHA256SUMS.txt'), $checksums)
    Compress-Archive -Path (Join-Path $stage '*') -DestinationPath $archive -CompressionLevel Optimal
    $archiveHash = Get-Sha256 $archive
    & powershell -NoProfile -ExecutionPolicy Bypass -File (Join-Path $PSScriptRoot 'install-local.ps1') -SourceDir $stage
    if ($LASTEXITCODE -ne 0) { throw 'local install failed' }
    Write-Output "VOIDSPACE_PACKAGE_OK $archive $archiveHash"
}
finally {
    Pop-Location
}
