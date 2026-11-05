# Builds the Microsoft Store MSIX package for EarDeck.
#
#   .\store\msix\build-msix.ps1            build the app, then package it
#   .\store\msix\build-msix.ps1 -NoBuild   package the existing release exe
#
# Writes store\msix\out\EarDeck_<version>_x64.msix. Upload that file in Partner Center.
# The Store signs it, so it is left unsigned here.
param([switch]$NoBuild)
$ErrorActionPreference = 'Stop'

$root = Resolve-Path "$PSScriptRoot\..\.."
$msix = "$root\store\msix"
$cfg = Get-Content "$msix\msix.config.json" -Raw | ConvertFrom-Json
foreach ($k in 'IdentityName', 'Publisher', 'PublisherDisplayName') {
    if (-not $cfg.$k -or $cfg.$k -like '*FROM_PARTNER_CENTER*') {
        throw "Set $k in store\msix\msix.config.json from Partner Center > Product identity."
    }
}

# Store versions are four numbers and the last one must be 0.
$version = (Select-String -Path "$root\Cargo.toml" -Pattern '^version = "(.+)"').Matches[0].Groups[1].Value
$storeVersion = "$version.0"

$sdk = Get-ChildItem "${env:ProgramFiles(x86)}\Windows Kits\10\bin\10.*\x64\makeappx.exe" |
    Sort-Object FullName -Descending | Select-Object -First 1
if (-not $sdk) { throw 'Install the Windows SDK to get makeappx.exe and makepri.exe.' }
$bin = $sdk.DirectoryName

if (-not $NoBuild) {
    # cargo reports progress on stderr, which Windows PowerShell treats as an error under Stop.
    Push-Location $root
    $ErrorActionPreference = 'Continue'
    try { cargo build --release; if ($LASTEXITCODE) { throw 'cargo build failed' } }
    finally { $ErrorActionPreference = 'Stop'; Pop-Location }
}
$exe = "$root\target\release\eardeck.exe"
if (-not (Test-Path $exe)) { throw "Missing $exe. Run without -NoBuild." }

# Lay out the package: the exe, its tile assets, and the filled-in manifest.
$layout = "$msix\out\layout"
Remove-Item $layout -Recurse -Force -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force "$layout\Assets" | Out-Null
Copy-Item $exe $layout
Copy-Item "$msix\Assets\*.png" "$layout\Assets"

$manifest = (Get-Content "$msix\AppxManifest.xml" -Raw).
    Replace('{{IdentityName}}', [Security.SecurityElement]::Escape($cfg.IdentityName)).
    Replace('{{Publisher}}', [Security.SecurityElement]::Escape($cfg.Publisher)).
    Replace('{{PublisherDisplayName}}', [Security.SecurityElement]::Escape($cfg.PublisherDisplayName)).
    Replace('{{Version}}', $storeVersion)
[IO.File]::WriteAllText("$layout\AppxManifest.xml", $manifest)

# resources.pri lets Windows pick the right scale and size of each asset.
Push-Location $layout
try {
    & "$bin\makepri.exe" createconfig /cf priconfig.xml /dq en-US /pv 10.0.0 /o | Out-Null
    # The default config splits scales into separate resource packs, which only bundles support.
    # One .msix needs every asset in a single resources.pri.
    $pri = [xml](Get-Content priconfig.xml -Raw)
    $pack = $pri.SelectSingleNode('//packaging')
    if ($pack) { [void]$pack.ParentNode.RemoveChild($pack) }
    $pri.Save("$layout\priconfig.xml")
    & "$bin\makepri.exe" new /pr . /cf priconfig.xml /mn AppxManifest.xml /of resources.pri /o | Out-Null
    if ($LASTEXITCODE) { throw 'makepri failed' }
    Remove-Item priconfig.xml
} finally { Pop-Location }

$package = "$msix\out\EarDeck_${version}_x64.msix"
& "$bin\makeappx.exe" pack /d $layout /p $package /o
if ($LASTEXITCODE) { throw 'makeappx failed' }
Write-Host "Built $package (version $storeVersion)"
