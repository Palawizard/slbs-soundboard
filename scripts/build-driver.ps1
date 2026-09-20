[CmdletBinding()]
param(
    [ValidateSet("Debug", "Release")]
    [string] $Configuration = "Debug",
    [ValidateSet("x64")]
    [string] $Platform = "x64"
)

$ErrorActionPreference = "Stop"
$workspace = Split-Path -Parent $PSScriptRoot
$driverRoot = Join-Path $workspace "native\driver\sysvad"
$project = Join-Path $driverRoot "SlbVirtualAudio\SlbVirtualAudio.vcxproj"
$endpointsProject = Join-Path $driverRoot "EndpointsCommon\EndpointsCommon.vcxproj"
$vswhere = Join-Path ${env:ProgramFiles(x86)} "Microsoft Visual Studio\Installer\vswhere.exe"

if (-not (Test-Path -LiteralPath $vswhere)) {
    throw "Visual Studio Installer was not found."
}

$installation = & $vswhere -latest -products * -requires Component.Microsoft.Windows.DriverKit.BuildTools -property installationPath
if (-not $installation) {
    throw "Visual Studio with the Windows Driver Kit component was not found."
}

$msbuild = Join-Path $installation "MSBuild\Current\Bin\MSBuild.exe"
$kitRoot = Join-Path ${env:ProgramFiles(x86)} "Windows Kits\10"
$sdkVersion = Get-ChildItem (Join-Path $kitRoot "Include") -Directory |
    Where-Object { $_.Name -match '^10\.0\.\d+\.0$' } |
    Sort-Object { [version]$_.Name } -Descending |
    Select-Object -First 1 -ExpandProperty Name

if (-not $sdkVersion) {
    throw "A Windows 10/11 WDK was not found."
}

$properties = @(
    "/p:Configuration=$Configuration",
    "/p:Platform=$Platform",
    "/p:WindowsTargetPlatformVersion=$sdkVersion",
    "/p:SkipPackageVerification=true"
)

& $msbuild $endpointsProject /m /t:Build @properties /v:minimal
if ($LASTEXITCODE -ne 0) { throw "EndpointsCommon build failed." }

& $msbuild $project /m /t:Build @properties /v:minimal
if ($LASTEXITCODE -ne 0) { throw "SLB virtual audio driver build failed." }

$output = Join-Path (Split-Path $project -Parent) "$Platform\$Configuration"
$inf = Join-Path $output "SlbVirtualAudio.inf"
$infverif = Join-Path $kitRoot "Tools\$sdkVersion\x64\infverif.exe"
& $infverif /v $inf
if ($LASTEXITCODE -ne 0) { throw "INF validation failed." }

$package = Join-Path $workspace "native\driver\package\$Platform\$Configuration"
New-Item -ItemType Directory -Force -Path $package | Out-Null
Copy-Item -Force -LiteralPath (Join-Path $output "SlbVirtualAudio.sys") -Destination $package
Copy-Item -Force -LiteralPath $inf -Destination $package

$inf2cat = Join-Path $kitRoot "bin\$sdkVersion\x86\Inf2Cat.exe"
& $inf2cat "/driver:$package" /os:10_X64
if ($LASTEXITCODE -ne 0) { throw "Catalog generation failed." }

$catalog = Join-Path $package "SlbVirtualAudio.cat"
if ($Configuration -eq "Debug") {
    $driverSignature = Get-AuthenticodeSignature (Join-Path $package "SlbVirtualAudio.sys")
    if (-not $driverSignature.SignerCertificate) {
        throw "The WDK did not test-sign the debug driver."
    }

    $signTool = Join-Path $kitRoot "bin\$sdkVersion\x64\signtool.exe"
    $thumbprint = $driverSignature.SignerCertificate.Thumbprint
    & $signTool sign /sha1 $thumbprint /fd SHA256 $catalog
    if ($LASTEXITCODE -ne 0) { throw "Debug catalog signing failed." }
    Export-Certificate -Cert $driverSignature.SignerCertificate -FilePath (Join-Path $package "SlbVirtualAudioTest.cer") -Force | Out-Null
}

Write-Host "Driver package created at $package"
