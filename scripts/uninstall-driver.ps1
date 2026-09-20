[CmdletBinding()]
param()

$ErrorActionPreference = "Stop"
$identity = [Security.Principal.WindowsIdentity]::GetCurrent()
$principal = [Security.Principal.WindowsPrincipal]::new($identity)
if (-not $principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)) {
    throw "Run this script from an elevated PowerShell session."
}

$devices = Get-PnpDevice | Where-Object { $_.InstanceId -like "ROOT\SLBVIRTUALMICROPHONE\*" }
foreach ($device in $devices) {
    & pnputil.exe /remove-device $device.InstanceId
    if ($LASTEXITCODE -ne 0) { throw "Could not remove $($device.InstanceId)." }
}

$publishedNames = Get-WindowsDriver -Online |
    Where-Object { $_.OriginalFileName -like "*SlbVirtualAudio.inf" } |
    Select-Object -ExpandProperty Driver
foreach ($publishedName in $publishedNames) {
    & pnputil.exe /delete-driver $publishedName /uninstall
    if ($LASTEXITCODE -ne 0) { throw "Could not remove $publishedName from the driver store." }
}

Write-Host "SLB Virtual Microphone uninstalled."
