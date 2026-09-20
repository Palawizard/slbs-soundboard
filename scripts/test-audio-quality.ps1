[CmdletBinding()]
param()

$ErrorActionPreference = "Stop"
$workspace = Split-Path -Parent $PSScriptRoot

& cargo test --manifest-path (Join-Path $workspace "Cargo.toml") -p slb-audio-engine
if ($LASTEXITCODE -ne 0) { throw "Audio unit and stability tests failed." }

& cargo run --manifest-path (Join-Path $workspace "Cargo.toml") -p slb-audio-engine --example audio_quality_probe
if ($LASTEXITCODE -ne 0) { throw "Audio quality thresholds failed." }

& (Join-Path $PSScriptRoot "build-driver.ps1") -Configuration Debug
if ($LASTEXITCODE -ne 0) { throw "Virtual audio driver verification failed." }
