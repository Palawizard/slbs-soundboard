[CmdletBinding()]
param([Parameter(Mandatory = $true)][string] $Path)

$ErrorActionPreference = "Stop"
$thumbprint = $env:SLB_SIGN_CERT_SHA1
if (-not $thumbprint -or $thumbprint -notmatch '^[A-Fa-f0-9]{40}$') {
    throw "SLB_SIGN_CERT_SHA1 must contain the 40-character SHA-1 thumbprint of the application signing certificate."
}

$kits = Join-Path ${env:ProgramFiles(x86)} "Windows Kits\10\bin"
$signTool = Get-ChildItem -LiteralPath $kits -Recurse -Filter signtool.exe |
    Where-Object { $_.Directory.Name -eq "x64" } |
    Sort-Object { [version]$_.Directory.Parent.Name } -Descending |
    Select-Object -First 1 -ExpandProperty FullName
if (-not $signTool) { throw "signtool.exe was not found in the Windows SDK." }

& $signTool sign /sha1 $thumbprint /fd SHA256 /tr http://timestamp.digicert.com /td SHA256 $Path
if ($LASTEXITCODE -ne 0) { throw "Authenticode signing failed for $Path." }

$signature = Get-AuthenticodeSignature -LiteralPath $Path
if ($signature.Status -ne "Valid") { throw "The resulting Authenticode signature is not valid for $Path." }
