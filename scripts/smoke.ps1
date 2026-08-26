$ErrorActionPreference = 'Stop'

$repo = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
$smokeBase = Join-Path ([System.IO.Path]::GetTempPath()) ("voidspace-smoke-" + [guid]::NewGuid().ToString('N'))
$root = Join-Path $smokeBase 'root'
$out1 = Join-Path $smokeBase 'out-before'
$out2 = Join-Path $smokeBase 'out-after'
$deleteTarget = Join-Path $root 'delete-only-this'

try {
    $typography = & cargo run --quiet -p voidspace-app --bin voidspace-smoke --release -- --typography
    if ($LASTEXITCODE -ne 0) { throw 'embedded typography smoke failed' }
    if ($typography -notmatch '^typography_source=embedded-selected ') { throw 'selected typography was not active' }
    Write-Output $typography

    New-Item -ItemType Directory -Force $root, $out1, $out2, $deleteTarget | Out-Null
    [System.IO.File]::WriteAllBytes((Join-Path $root 'alpha.bin'), [byte[]]::new(8192))
    [System.IO.File]::WriteAllText((Join-Path $deleteTarget 'gone.txt'), 'temporary')

    $beforeJson = & cargo run --quiet -p voidspace-app --bin voidspace-smoke --release -- $root $out1
    if ($LASTEXITCODE -ne 0) { throw 'initial smoke scan failed' }
    $before = $beforeJson | ConvertFrom-Json

    [System.IO.File]::WriteAllBytes((Join-Path $root 'appeared.bin'), [byte[]]::new(16384))
    $afterJson = & cargo run --quiet -p voidspace-app --bin voidspace-smoke --release -- $root $out2
    if ($LASTEXITCODE -ne 0) { throw 'post-mutation smoke scan failed' }
    $after = $afterJson | ConvertFrom-Json
    if ($after.files -le $before.files) { throw 'mutation was not reflected by a fresh scan' }

    & cargo run --quiet -p voidspace-app --bin voidspace-smoke --release -- --delete $root $deleteTarget | Out-Null
    if ($LASTEXITCODE -ne 0) { throw 'guarded permanent-delete smoke failed' }
    if (Test-Path -LiteralPath $deleteTarget) { throw 'guarded deletion left its target behind' }
    if (-not (Test-Path -LiteralPath (Join-Path $root 'alpha.bin'))) { throw 'guarded deletion escaped its target' }

    foreach ($artifact in 'scan.voidspace', 'report.csv', 'report.json', 'report.html', 'report.txt') {
        if (-not (Test-Path -LiteralPath (Join-Path $out2 $artifact))) { throw "missing artifact: $artifact" }
    }
    Write-Output 'VOIDSPACE_SMOKE_OK'
}
finally {
    if (Test-Path -LiteralPath $smokeBase) {
        $resolved = (Resolve-Path -LiteralPath $smokeBase).Path
        $tempRoot = [System.IO.Path]::GetFullPath([System.IO.Path]::GetTempPath())
        if (-not $resolved.StartsWith($tempRoot, [System.StringComparison]::OrdinalIgnoreCase)) {
            throw 'refusing to clean a smoke path outside the temp directory'
        }
        Remove-Item -LiteralPath $resolved -Recurse -Force
    }
}
