param([Parameter(Mandatory)][string]$SourceDir)
$ErrorActionPreference = 'Stop'

function Get-Sha256([string]$Path) {
    return (Get-FileHash -Algorithm SHA256 -LiteralPath $Path).Hash.ToLowerInvariant()
}

$source = (Resolve-Path -LiteralPath $SourceDir).Path
if (-not (Test-Path -LiteralPath $source -PathType Container)) {
    throw "Package source is not a directory: $source"
}
$installDir = Join-Path ([Environment]::GetFolderPath('LocalApplicationData')) 'Voidspace'
$installExe = Join-Path $installDir 'voidspace.exe'
$installedExecutables = @(
    $installExe,
    (Join-Path $installDir 'voidspace-elevated.exe'),
    "$installExe.old",
    "$(Join-Path $installDir 'voidspace-elevated.exe').old"
) |
    ForEach-Object { [IO.Path]::GetFullPath($_) }
$desktop = [Environment]::GetFolderPath('Desktop')
$shortcutPath = Join-Path $desktop 'Voidspace.lnk'
$rootFiles = @(
    'voidspace.exe',
    'voidspace-elevated.exe',
    'README.md',
    'LICENSE',
    'SHA256SUMS.txt',
    'THIRD-PARTY-FONTS.txt'
)
$requiredNotices = @(
    'licenses\fonts\unbounded\OFL.txt',
    'licenses\fonts\golostext\OFL.txt',
    'licenses\fonts\jetbrainsmono\OFL.txt',
    'licenses\fonts\hack\Hack-Regular.txt',
    'licenses\fonts\ubuntu\UFL.txt'
)

foreach ($name in $rootFiles) {
    $candidate = Join-Path $source $name
    if (-not (Test-Path -LiteralPath $candidate -PathType Leaf)) {
        throw "Missing packaged file: $name"
    }
}
foreach ($relative in $requiredNotices) {
    $candidate = Join-Path $source $relative
    if (-not (Test-Path -LiteralPath $candidate -PathType Leaf)) {
        throw "Missing packaged font notice: $relative"
    }
}

$running = Get-CimInstance Win32_Process | Where-Object {
    if (-not $_.ExecutablePath) { return $false }
    $processPath = [IO.Path]::GetFullPath($_.ExecutablePath)
    return $installedExecutables -contains $processPath
}
$running | ForEach-Object {
    Stop-Process -Id $_.ProcessId -Force
    Wait-Process -Id $_.ProcessId -Timeout 10 -ErrorAction SilentlyContinue
}

New-Item -ItemType Directory -Force -Path $installDir | Out-Null
foreach ($name in $rootFiles) {
    $from = Join-Path $source $name
    $to = Join-Path $installDir $name
    $next = "$to.new"
    $old = "$to.old"
    if (Test-Path -LiteralPath $next) { Remove-Item -LiteralPath $next -Force }
    if (Test-Path -LiteralPath $old) { Remove-Item -LiteralPath $old -Force }
    Copy-Item -LiteralPath $from -Destination $next -Force
    if ((Get-Sha256 $from) -ne (Get-Sha256 $next)) {
        throw "Staged install copy mismatch: $name"
    }
    if (Test-Path -LiteralPath $to) {
        [IO.File]::Replace($next, $to, $old)
        if ((Get-Sha256 $from) -ne (Get-Sha256 $to)) {
            throw "Installed root file mismatch: $name"
        }
        Remove-Item -LiteralPath $old -Force
    }
    else {
        Move-Item -LiteralPath $next -Destination $to
    }
}

$licenseSource = Join-Path $source 'licenses'
$licenseCurrent = Join-Path $installDir 'licenses'
$licenseNext = Join-Path $installDir 'licenses.new'
$licenseOld = Join-Path $installDir 'licenses.old'
$installPrefix = [IO.Path]::GetFullPath($installDir + [IO.Path]::DirectorySeparatorChar)
foreach ($candidate in @($licenseNext, $licenseOld)) {
    $full = [IO.Path]::GetFullPath($candidate)
    if (-not $full.StartsWith($installPrefix, [StringComparison]::OrdinalIgnoreCase)) {
        throw "Unsafe license path: $full"
    }
    if (Test-Path -LiteralPath $full) {
        Remove-Item -LiteralPath $full -Recurse -Force
    }
}
Copy-Item -LiteralPath $licenseSource -Destination $licenseNext -Recurse
foreach ($relative in $requiredNotices) {
    $from = Join-Path $source $relative
    $next = Join-Path $licenseNext ($relative -replace '^licenses\\', '')
    if (-not (Test-Path -LiteralPath $next -PathType Leaf)) {
        throw "Missing copied font notice: $relative"
    }
    if ((Get-Sha256 $from) -ne (Get-Sha256 $next)) {
        throw "Copied font notice mismatch: $relative"
    }
}
if (Test-Path -LiteralPath $licenseCurrent) {
    Move-Item -LiteralPath $licenseCurrent -Destination $licenseOld
}
Move-Item -LiteralPath $licenseNext -Destination $licenseCurrent
if (Test-Path -LiteralPath $licenseOld) {
    Remove-Item -LiteralPath $licenseOld -Recurse -Force
}

$shell = New-Object -ComObject WScript.Shell
$shortcut = $shell.CreateShortcut($shortcutPath)
$shortcut.TargetPath = $installExe
$shortcut.WorkingDirectory = $installDir
$shortcut.IconLocation = "$installExe,0"
$shortcut.Save()

$readBack = $shell.CreateShortcut($shortcutPath)
if ([IO.Path]::GetFullPath($readBack.TargetPath) -ne [IO.Path]::GetFullPath($installExe)) {
    throw 'Desktop shortcut target mismatch'
}
if ([IO.Path]::GetFullPath($readBack.WorkingDirectory) -ne [IO.Path]::GetFullPath($installDir)) {
    throw 'Desktop shortcut working directory mismatch'
}
$packageHash = Get-Sha256 (Join-Path $source 'voidspace.exe')
$installedHash = Get-Sha256 $installExe
if ($packageHash -ne $installedHash) { throw 'Installed executable hash mismatch' }
foreach ($relative in $requiredNotices) {
    $staged = Join-Path $source $relative
    $installed = Join-Path $installDir $relative
    if (-not (Test-Path -LiteralPath $installed -PathType Leaf)) {
        throw "Missing installed notice: $relative"
    }
    if ((Get-Sha256 $staged) -ne (Get-Sha256 $installed)) {
        throw "Installed notice mismatch: $relative"
    }
}

Write-Output "VOIDSPACE_DESKTOP_OK $shortcutPath $installExe $installedHash"
