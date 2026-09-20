[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)][string] $Msi,
    [string] $UpgradeMsi,
    [switch] $Execute
)

$ErrorActionPreference = "Stop"
$msiPath = (Resolve-Path -LiteralPath $Msi).Path
$upgradePath = if ($UpgradeMsi) { (Resolve-Path -LiteralPath $UpgradeMsi).Path } else { $null }
$signature = Get-AuthenticodeSignature -LiteralPath $msiPath
if ($signature.Status -ne "Valid" -and $Execute) { throw "Executable lifecycle tests require a valid signed MSI." }

if (-not $Execute) {
    Write-Host "Installer validation passed. Use -Execute on a disposable elevated Windows VM to run install, upgrade and uninstall."
    exit 0
}

$library = Join-Path $env:APPDATA "fr.slb.soundboard\library.sqlite3"
& msiexec.exe /i $msiPath /qn /norestart
if ($LASTEXITCODE -ne 0) { throw "Initial MSI installation failed." }
$before = if (Test-Path -LiteralPath $library) { (Get-FileHash -LiteralPath $library -Algorithm SHA256).Hash } else { $null }

if ($upgradePath) {
    & msiexec.exe /i $upgradePath /qn /norestart
    if ($LASTEXITCODE -ne 0) { throw "MSI upgrade failed." }
    if ($before -and (Get-FileHash -LiteralPath $library -Algorithm SHA256).Hash -ne $before) { throw "The upgrade changed the user library." }
}

$product = Get-CimInstance Win32_Product | Where-Object { $_.Name -eq "SLB's Soundboard" } | Select-Object -First 1
if (-not $product) { throw "The installed MSI product was not found." }
& msiexec.exe /x $product.IdentifyingNumber /qn /norestart
if ($LASTEXITCODE -ne 0) { throw "MSI uninstall failed." }
if ($before -and -not (Test-Path -LiteralPath $library)) { throw "Uninstall removed the user library." }
$device = & pnputil.exe /enum-devices /instanceid "ROOT\SLBVIRTUALMICROPHONE\0000"
if ($device -match 'SLBVIRTUALMICROPHONE') { throw "Uninstall left the virtual device installed." }
Write-Host "Installer lifecycle passed: install, optional upgrade, library preservation and driver removal."
