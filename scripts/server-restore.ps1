[CmdletBinding()]
param(
    [Parameter(Mandatory)][string]$BackupDirectory,
    [switch]$ConfirmRestore
)

$ErrorActionPreference = "Stop"
$PSNativeCommandUseErrorActionPreference = $true
if (!$ConfirmRestore) { throw "Pass -ConfirmRestore to acknowledge replacement of current server data." }
$backupRoot = [System.IO.Path]::GetFullPath($BackupDirectory)
$databaseFile = Join-Path $backupRoot "database.dump"
$mediaFile = Join-Path $backupRoot "media.tar.gz"
$manifest = Get-Content -Raw -LiteralPath (Join-Path $backupRoot "manifest.json") | ConvertFrom-Json
if ((Get-FileHash -Algorithm SHA256 -LiteralPath $databaseFile).Hash.ToLowerInvariant() -ne $manifest.databaseSha256) { throw "Database archive checksum mismatch." }
if ((Get-FileHash -Algorithm SHA256 -LiteralPath $mediaFile).Hash.ToLowerInvariant() -ne $manifest.mediaSha256) { throw "Media archive checksum mismatch." }

docker compose stop cloudflared api 2>$null
$postgresContainer = (docker compose ps -q postgres).Trim()
if (!$postgresContainer) { throw "PostgreSQL container must be running." }
docker cp $databaseFile "$postgresContainer`:/tmp/slb-database.dump"
docker compose exec -T postgres dropdb -U ($env:POSTGRES_USER ?? "slb") --if-exists --force ($env:POSTGRES_DB ?? "slb_community")
docker compose exec -T postgres createdb -U ($env:POSTGRES_USER ?? "slb") -T template0 ($env:POSTGRES_DB ?? "slb_community")
docker compose exec -T postgres pg_restore -U ($env:POSTGRES_USER ?? "slb") -d ($env:POSTGRES_DB ?? "slb_community") --exit-on-error --clean --if-exists /tmp/slb-database.dump
docker compose exec -T postgres rm -f /tmp/slb-database.dump

docker compose run --rm --no-deps --user root -v "${backupRoot}:/backup:ro" api sh -c "find /data/media -mindepth 1 -delete && tar -xzf /backup/media.tar.gz -C /data/media && chown -R node:node /data/media"
if ($LASTEXITCODE -ne 0) { throw "Media restore failed." }
docker compose up -d api
Write-Host "Restore completed. Run scripts/server-smoke.ps1 before reopening public traffic."
