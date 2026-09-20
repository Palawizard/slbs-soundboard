# Community service deployment

The production stack contains PostgreSQL, the Fastify API, persistent media storage, and an optional Cloudflare Tunnel connector. PostgreSQL is reachable only on the internal Docker network. The API port is bound to loopback for local operations; public traffic should use the tunnel.

## Initial setup

1. Copy `.env.example` to `.env` and replace every placeholder with a random secret or the corresponding Google/Cloudflare value.
2. Create a Google OAuth client of type **Desktop app**. Configure the desktop client to open the returned authorization URL and listen on an ephemeral `127.0.0.1` callback port.
3. Create a remotely managed Cloudflare Tunnel and map the public hostname to `http://api:3000`.
4. Start the origin with `docker compose --profile tunnel up -d --build`.
5. Confirm `docker compose ps` reports PostgreSQL and the API as healthy, then call `/health` through the public hostname.

For a locally managed tunnel, mount a completed copy of `deploy/cloudflared/config.yml.example` and its credentials JSON instead of the token command. Keep the final catch-all `http_status:404` ingress rule.

## Operations

- `scripts/server-smoke.ps1` builds the stack, waits for readiness, and validates migrations and local health. Pass `-SkipBuild` to verify an image that is already present without contacting the registry.
- `scripts/server-backup.ps1 -Destination <directory>` creates a compressed PostgreSQL custom archive, a media archive, and a SHA-256 manifest.
- `scripts/server-restore.ps1 -BackupDirectory <directory> -ConfirmRestore` verifies the manifest before replacing database and media contents.

Take backups while publishing is paused to keep database metadata and media files at the same point in time. Store backup copies outside the Docker host and test restoration regularly.
