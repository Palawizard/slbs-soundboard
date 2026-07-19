import { randomBytes } from "node:crypto";
import { mkdtemp, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { Pool } from "pg";
import { afterAll, beforeAll, describe, expect, it } from "vitest";
import { migrateDatabase } from "../src/database/migrate.js";
import { PostgresCommunityRepository } from "../src/database/postgres.js";

const databaseUrl = process.env.TEST_DATABASE_URL;
const schema = `slb_epic4_${randomBytes(6).toString("hex")}`;
let bootstrap: Pool;
let repository: PostgresCommunityRepository;

describe.skipIf(!databaseUrl)("PostgreSQL community repository", () => {
  beforeAll(async () => {
    bootstrap = new Pool({ connectionString: databaseUrl });
    await bootstrap.query(`CREATE SCHEMA ${schema}`);
    const isolatedUrl = new URL(databaseUrl!);
    isolatedUrl.searchParams.set("options", `-c search_path=${schema}`);
    repository = new PostgresCommunityRepository(isolatedUrl.toString());
    await migrateDatabase(repository.pool);
  }, 30_000);

  afterAll(async () => {
    await repository?.close();
    if (bootstrap) {
      await bootstrap.query(`DROP SCHEMA IF EXISTS ${schema} CASCADE`);
      await bootstrap.end();
    }
  });

  it("applies migrations idempotently and rolls a failed migration back", async () => {
    await migrateDatabase(repository.pool);
    const applied = await repository.pool.query<{ version: string }>("SELECT version FROM schema_migrations ORDER BY version");
    expect(applied.rows.map((row) => row.version)).toEqual(["001_community.sql", "002_integrity.sql"]);

    const root = await mkdtemp(join(tmpdir(), "slb-migration-"));
    try {
      await writeFile(join(root, "900_failure.sql"), "CREATE TABLE must_rollback(id integer); SELECT missing_column FROM must_rollback;");
      await expect(migrateDatabase(repository.pool, root)).rejects.toThrow();
      const rolledBack = await repository.pool.query<{ table_name: string | null }>("SELECT to_regclass('must_rollback')::text AS table_name");
      expect(rolledBack.rows[0]?.table_name).toBeNull();
      expect((await repository.pool.query("SELECT 1 FROM schema_migrations WHERE version = '900_failure.sql'")).rowCount).toBe(0);
    } finally {
      await rm(root, { recursive: true, force: true });
    }
  });

  it("persists sessions, ownership, publications, search and audits", async () => {
    const first = await repository.upsertGoogleUser({ subject: "google-first", email: "first@example.test", emailVerified: true, name: "Premier", avatarUrl: null });
    const second = await repository.upsertGoogleUser({ subject: "google-second", email: "second@example.test", emailVerified: true, name: "Second", avatarUrl: null });
    const tokenHash = randomBytes(32);
    await repository.createSession(first.id, tokenHash, new Date(Date.now() + 60_000));
    expect((await repository.authenticateSession(tokenHash, new Date()))?.id).toBe(first.id);

    const audio = await repository.registerOwnedMedia(first.id, {
      kind: "audio", hash: "a".repeat(64), mimeType: "audio/wav", extension: "wav", byteSize: 16_044,
      durationMs: 1_000, width: null, height: null, storageKey: `audio/aa/${"a".repeat(64)}.wav`,
    });
    const duplicate = await repository.registerOwnedMedia(first.id, {
      kind: "audio", hash: "a".repeat(64), mimeType: "audio/wav", extension: "wav", byteSize: 16_044,
      durationMs: 1_000, width: null, height: null, storageKey: `audio/aa/${"a".repeat(64)}.wav`,
    });
    expect(duplicate.id).toBe(audio.id);

    const publication = await repository.createPublication(first.id, { title: "Cloche bleue", description: "Un son communautaire", audioMediaId: audio.id, imageMediaId: null });
    expect((await repository.listPublications({ query: "cloche", limit: 10 })).map((row) => row.id)).toContain(publication.id);
    await expect(repository.createPublication(second.id, { title: "Copie", description: "", audioMediaId: audio.id, imageMediaId: null })).rejects.toMatchObject({ code: "forbidden" });

    await expect(repository.pool.query(
      "INSERT INTO publications(id, owner_id, audio_media_id, title) VALUES (gen_random_uuid(), $1, $2, 'Contournement')",
      [second.id, audio.id],
    )).rejects.toMatchObject({ code: "23514" });

    await repository.writeAudit({ actorUserId: first.id, action: "integration.test", targetType: "publication", targetId: publication.id, requestId: "test-request", sourceIp: "127.0.0.1" });
    expect((await repository.pool.query("SELECT 1 FROM audit_events WHERE action = 'integration.test'")).rowCount).toBe(1);
  });
});
