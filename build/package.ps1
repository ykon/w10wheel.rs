<#
.SYNOPSIS
    Builds W10Wheel.exe and packages it with its documents into a zip, with its SHA-256.

.DESCRIPTION
    The version is that of Cargo.toml (the one source: the exe, its version information and About
    use it). dist/Readme.ja.txt must have the same version and a history line for it.
    Written to artifacts/:
      W10Wheel.rs-<version>.zip         W10Wheel.exe, Readme.ja.txt, License.txt, Library.txt,
                                        AccelTable.txt (in a folder of the same name)
      W10Wheel.rs-<version>.zip.sha256  "<hash>  <zip name>" (as sha256sum writes it)
    Used both locally and by .github/workflows/build.yml.

.PARAMETER Offline
    Builds with the crates already downloaded (cargo --offline), as the local builds do.

.PARAMETER SkipBuild
    Packages target/release/W10Wheel.exe as it is (built before).

.EXAMPLE
    pwsh build/package.ps1 -Offline
#>
#Requires -Version 7
# PowerShell 7 (pwsh): this file is UTF-8 without a BOM (it matches Japanese text of the Readme)
param(
    [string]$OutDir,
    [switch]$Offline,
    [switch]$SkipBuild
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$root = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
if (-not $OutDir) { $OutDir = Join-Path $root 'artifacts' }

# The version of the package: the first "version" of Cargo.toml (in [package])
$cargoToml = Get-Content (Join-Path $root 'Cargo.toml') -Raw
if ($cargoToml -notmatch '(?m)^version\s*=\s*"([^"]+)"') {
    throw 'version not found in Cargo.toml'
}
$version = $Matches[1]
Write-Host "Version: $version"

# The Readme tells the same version, and what it brings (as the Java version checks it)
$readmePath = Join-Path $root 'dist/Readme.ja.txt'
$readme = [System.IO.File]::ReadAllText($readmePath, [System.Text.Encoding]::UTF8)
$readmeVersion = [regex]::Match($readme, '(?m)^バージョン:\r?\n\s+(\S+)\s*$').Groups[1].Value
if ($readmeVersion -ne $version) {
    throw "Version mismatch: dist/Readme.ja.txt has '$readmeVersion', Cargo.toml has '$version'."
}
if (-not $readme.Contains(": v${version}:")) {
    throw "dist/Readme.ja.txt has no history line for v${version}."
}

if (-not $SkipBuild) {
    $cargoArgs = @('build', '--release', '--locked')
    if ($Offline) { $cargoArgs += '--offline' }
    Push-Location $root
    try {
        & cargo @cargoArgs | Out-Host
        if ($LASTEXITCODE -ne 0) { throw "cargo build failed (exit code $LASTEXITCODE)" }
    }
    finally {
        Pop-Location
    }
}

$exe = Join-Path $root 'target/release/W10Wheel.exe'
if (-not (Test-Path $exe)) { throw "Not built: $exe" }
# The exe of this version (e.g. not one built before Cargo.toml was changed, with -SkipBuild)
$exeVersion = [System.Diagnostics.FileVersionInfo]::GetVersionInfo($exe).ProductVersion
if ($exeVersion -ne $version) {
    throw "W10Wheel.exe is version '$exeVersion', not '$version': build it again."
}

$name = "W10Wheel.rs-$version"
$staging = Join-Path $OutDir $name
$zip = Join-Path $OutDir "$name.zip"
$sha = "$zip.sha256"

foreach ($old in $staging, $zip, $sha) {
    if (Test-Path $old) { Remove-Item $old -Recurse -Force }
}
New-Item $staging -ItemType Directory -Force | Out-Null

Copy-Item $exe $staging
# By name: dist also holds what is not distributed (release-notes)
Copy-Item (Join-Path $root 'dist/Readme.ja.txt') $staging
Copy-Item (Join-Path $root 'License.txt') $staging
Copy-Item (Join-Path $root 'Library.txt') $staging
Copy-Item (Join-Path $root 'AccelTable.txt') $staging

Compress-Archive -Path $staging -DestinationPath $zip -CompressionLevel Optimal

$hash = (Get-FileHash -Path $zip -Algorithm SHA256).Hash.ToLowerInvariant()
[System.IO.File]::WriteAllText($sha, "$hash  $name.zip`n", [System.Text.UTF8Encoding]::new($false))

Write-Host ("Created: {0} ({1:N0} bytes)" -f $zip, (Get-Item $zip).Length)
Write-Host "Created: $sha ($hash)"

if ($env:GITHUB_OUTPUT) {
    "version=$version" | Out-File $env:GITHUB_OUTPUT -Append -Encoding utf8
    "zip=$zip" | Out-File $env:GITHUB_OUTPUT -Append -Encoding utf8
    "sha256=$sha" | Out-File $env:GITHUB_OUTPUT -Append -Encoding utf8
}
