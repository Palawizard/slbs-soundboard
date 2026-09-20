[CmdletBinding()]
param([Parameter(Mandatory)][string]$Destination)

$ErrorActionPreference = "Stop"
$PSNativeCommandUseErrorActionPreference = $true
$backupRoot = [System.IO.Path]::GetFullPath($Destination)
New-Item -ItemType Directory -Path $backupRoot -Force | Out-Null
$databaseFile = Join-Path $backupRoot "database.dump"
$mediaFile = Join-Path $backupRoot "media.tar.gz"
$postgresContainer = (docker compose ps -q postgres).Trim()
$apiContainer = (docker compose ps -q api).Trim()
if (!$postgresContainer -or !$apiContainer) { throw "PostgreSQL and API containers must be running." }

docker compose exec -T postgres pg_dump -U ($env:POSTGRES_USER ?? "slb") -d ($env:POSTGRES_DB ?? "slb_community") -Fc -f /tmp/slb-database.dump
if ($LASTEXITCODE -ne 0) { throw "Database backup failed." }
docker cp "$postgresContainer`:/tmp/slb-database.dump" $databaseFile
docker compose exec -T postgres rm -f /tmp/slb-database.dump

docker compose exec -T api tar -czf /tmp/slb-media.tar.gz -C /data/media .
if ($LASTEXITCODE -ne 0) { throw "Media backup failed." }
docker cp "$apiContainer`:/tmp/slb-media.tar.gz" $mediaFile
docker compose exec -T api rm -f /tmp/slb-media.tar.gz

$manifest = [ordered]@{
    createdAt = [DateTime]::UtcNow.ToString("o")
    databaseSha256 = (Get-FileHash -Algorithm SHA256 -LiteralPath $databaseFile).Hash.ToLowerInvariant()
    mediaSha256 = (Get-FileHash -Algorithm SHA256 -LiteralPath $mediaFile).Hash.ToLowerInvariant()
}
$manifest | ConvertTo-Json | Set-Content -LiteralPath (Join-Path $backupRoot "manifest.json") -Encoding utf8NoBOM
Write-Host "Backup created in $backupRoot"
