[CmdletBinding()]
param([switch] $ValidateOnly)

$ErrorActionPreference = "Stop"
$workspace = Split-Path -Parent $PSScriptRoot
$driver = Join-Path $workspace "native\driver\package\x64\Release"
$staging = Join-Path $workspace "apps\desktop\src-tauri\release-resources\driver"
$required = @("SlbVirtualAudio.inf", "SlbVirtualAudio.sys", "SlbVirtualAudio.cat")

if ($env:SLB_COMMUNITY_API_URL -notmatch '^https://') {
    throw "SLB_COMMUNITY_API_URL must be a production HTTPS URL."
}
if ($env:SLB_SIGN_CERT_SHA1 -notmatch '^[A-Fa-f0-9]{40}$') {
    throw "SLB_SIGN_CERT_SHA1 must identify the application signing certificate."
}
foreach ($name in $required) {
    $path = Join-Path $driver $name
    if (-not (Test-Path -LiteralPath $path -PathType Leaf)) { throw "Missing release driver file: $name" }
    if ($name -ne "SlbVirtualAudio.inf" -and (Get-AuthenticodeSignature -LiteralPath $path).Status -ne "Valid") {
        throw "The production driver signature is invalid: $name"
    }
}

if (-not $ValidateOnly) {
    New-Item -ItemType Directory -Force -Path $staging | Out-Null
    foreach ($name in $required) { Copy-Item -Force -LiteralPath (Join-Path $driver $name) -Destination $staging }
}

Write-Host "Release gate passed: HTTPS API, application certificate and signed driver package are ready."
