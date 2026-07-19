[CmdletBinding()]
param(
    [ValidateSet("Debug", "Release")]
    [string] $Configuration = "Debug"
)

$ErrorActionPreference = "Stop"
$identity = [Security.Principal.WindowsIdentity]::GetCurrent()
$principal = [Security.Principal.WindowsPrincipal]::new($identity)
if (-not $principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)) {
    throw "Run this script from an elevated PowerShell session."
}

$workspace = Split-Path -Parent $PSScriptRoot
$package = Join-Path $workspace "native\driver\package\x64\$Configuration"
$inf = Join-Path $package "SlbVirtualAudio.inf"
if (-not (Test-Path -LiteralPath $inf)) {
    throw "Driver package not found. Run scripts/build-driver.ps1 first."
}

if ($Configuration -eq "Release") {
    $signatures = @(
        Get-AuthenticodeSignature (Join-Path $package "SlbVirtualAudio.sys")
        Get-AuthenticodeSignature (Join-Path $package "SlbVirtualAudio.cat")
    )
    if ($signatures.Where({ $_.Status -ne "Valid" }).Count -ne 0) {
        throw "Release installation requires a valid production signature."
    }
} else {
    $certificate = Join-Path $package "SlbVirtualAudioTest.cer"
    if (-not (Test-Path -LiteralPath $certificate)) {
        throw "Debug test certificate not found. Rebuild the debug package."
    }
    Import-Certificate -FilePath $certificate -CertStoreLocation Cert:\LocalMachine\Root | Out-Null
    Import-Certificate -FilePath $certificate -CertStoreLocation Cert:\LocalMachine\TrustedPublisher | Out-Null
}

& pnputil.exe /add-driver $inf /install
if ($LASTEXITCODE -ne 0) { throw "Driver-store installation failed." }

$existing = Get-PnpDevice -PresentOnly | Where-Object { $_.InstanceId -eq "ROOT\SLBVIRTUALMICROPHONE\0000" }
if (-not $existing) {
    $kitRoot = Join-Path ${env:ProgramFiles(x86)} "Windows Kits\10\Tools"
    $devcon = Get-ChildItem $kitRoot -Recurse -Filter devcon.exe |
        Where-Object { $_.Directory.Name -eq "x64" } |
        Sort-Object { [version]$_.Directory.Parent.Name } -Descending |
        Select-Object -First 1 -ExpandProperty FullName
    if (-not $devcon) {
        throw "DevCon was not found in the WDK tools."
    }
    & $devcon install $inf "Root\SLBVirtualMicrophone"
    if ($LASTEXITCODE -ne 0) {
        throw "The package was staged, but the root device could not be created. Install DevCon from the WDK tools."
    }
}

Write-Host "SLB Virtual Microphone installed."
