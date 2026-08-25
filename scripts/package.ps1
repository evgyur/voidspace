$ErrorActionPreference = 'Stop'

$repo = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
$dist = Join-Path $repo 'dist'
$stage = Join-Path $dist 'Voidspace-0.1.0-windows-x64'
$archive = Join-Path $dist 'Voidspace-0.1.0-windows-x64.zip'

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
    Copy-Item -LiteralPath (Join-Path $repo 'target\release\voidspace.exe') -Destination $stage
    Copy-Item -LiteralPath (Join-Path $repo 'target\release\voidspace-elevated.exe') -Destination $stage
    Copy-Item -LiteralPath (Join-Path $repo 'README.md') -Destination $stage
    Copy-Item -LiteralPath (Join-Path $repo 'LICENSE') -Destination $stage

    $checksums = Get-ChildItem -LiteralPath $stage -File | Sort-Object Name | ForEach-Object {
        "$(Get-Sha256 $_.FullName)  $($_.Name)"
    }
    [System.IO.File]::WriteAllLines((Join-Path $stage 'SHA256SUMS.txt'), $checksums)
    Compress-Archive -Path (Join-Path $stage '*') -DestinationPath $archive -CompressionLevel Optimal
    $archiveHash = Get-Sha256 $archive
    Write-Output "VOIDSPACE_PACKAGE_OK $archive $archiveHash"
}
finally {
    Pop-Location
}
