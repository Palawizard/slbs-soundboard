[CmdletBinding()]
param(
    [int]$TimeoutSeconds = 120,
    [switch]$SkipBuild
)

$ErrorActionPreference = "Stop"
$PSNativeCommandUseErrorActionPreference = $true
$arguments = @("compose", "up", "-d")
if (!$SkipBuild) { $arguments += "--build" }
$arguments += @("postgres", "api")
& docker $arguments
if ($LASTEXITCODE -ne 0) { throw "Docker Compose startup failed." }

$deadline = [DateTime]::UtcNow.AddSeconds($TimeoutSeconds)
do {
    try {
        $health = Invoke-RestMethod -Uri "http://127.0.0.1:$($env:API_PORT ?? '3000')/health" -TimeoutSec 3
        if ($health.status -eq "ok" -and $health.database -eq "ready") { break }
    } catch { Start-Sleep -Seconds 2 }
} while ([DateTime]::UtcNow -lt $deadline)

if ([DateTime]::UtcNow -ge $deadline) {
    docker compose logs --no-color api postgres
    throw "Community API did not become ready in time."
}

docker compose exec -T postgres psql -U ($env:POSTGRES_USER ?? "slb") -d ($env:POSTGRES_DB ?? "slb_community") -v ON_ERROR_STOP=1 -c "SELECT version FROM schema_migrations ORDER BY version;"
if ($LASTEXITCODE -ne 0) { throw "Migration verification failed." }
Write-Host "Community service smoke test passed."
