param([switch]$Check)
$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName System.Net.Http
$commit = '6a003b5eb672dc8bf5bff5937cf5863f8b175445'
$root = (Resolve-Path -LiteralPath (Join-Path $PSScriptRoot '..')).Path
$assets = @(
    @{ Remote='ofl/unbounded/Unbounded%5Bwght%5D.ttf'; Local='crates/voidspace-app/assets/fonts/Unbounded[wght].ttf'; Bytes=778272; Sha='323b511be380c8d474ef030686b71aedde501f8d9cd46da558b7c40454372c3f' },
    @{ Remote='ofl/golostext/GolosText%5Bwght%5D.ttf'; Local='crates/voidspace-app/assets/fonts/GolosText[wght].ttf'; Bytes=184292; Sha='17bb58fb69aec2dfb047a2ebf52534023e9b688c97a6b7ac795b0a72912c2063' },
    @{ Remote='ofl/jetbrainsmono/JetBrainsMono%5Bwght%5D.ttf'; Local='crates/voidspace-app/assets/fonts/JetBrainsMono[wght].ttf'; Bytes=187208; Sha='48715a42ec242c21e9f02692891e147d022299a52e48d5e413e1a942193ffeda' },
    @{ Remote='ofl/unbounded/OFL.txt'; Local='crates/voidspace-app/assets/licenses/fonts/unbounded/OFL.txt'; Bytes=4392; Sha='31e5d4e83955e7103c34570dd49b0570ef490800bd65b42923c0dd02445263b3' },
    @{ Remote='ofl/golostext/OFL.txt'; Local='crates/voidspace-app/assets/licenses/fonts/golostext/OFL.txt'; Bytes=4394; Sha='ff532f9e8789f09a9fdffc3c0954eedfb0a48be77b2e2eb90f5f82e4f347f50c' },
    @{ Remote='ofl/jetbrainsmono/OFL.txt'; Local='crates/voidspace-app/assets/licenses/fonts/jetbrainsmono/OFL.txt'; Bytes=4399; Sha='b2fe5e8987594e9ffd1d2ca52a2f5d73eb8335243893c5d6254b5ad69269591d' }
)
$client = [System.Net.Http.HttpClient]::new()
try {
    foreach ($asset in $assets) {
        $path = Join-Path $root $asset.Local
        if (-not $Check) {
            $directory = Split-Path -Parent $path
            New-Item -ItemType Directory -Force -Path $directory | Out-Null
            $url = "https://raw.githubusercontent.com/google/fonts/$commit/$($asset.Remote)"
            [IO.File]::WriteAllBytes($path, $client.GetByteArrayAsync($url).GetAwaiter().GetResult())
        }
        if (-not (Test-Path -LiteralPath $path -PathType Leaf)) { throw "Missing font asset: $path" }
        if ((Get-Item -LiteralPath $path).Length -ne $asset.Bytes) { throw "Size mismatch: $path" }
        $hash = (Get-FileHash -Algorithm SHA256 -LiteralPath $path).Hash.ToLowerInvariant()
        if ($hash -ne $asset.Sha) { throw "SHA-256 mismatch: $path" }
    }
} finally { $client.Dispose() }
Write-Output 'VOIDSPACE_FONT_ASSETS_OK'
