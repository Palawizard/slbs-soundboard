import { readdir, readFile } from "node:fs/promises";
import { fileURLToPath } from "node:url";
import { join } from "node:path";
import type { Pool, PoolClient } from "pg";

const defaultMigrationsRoot = fileURLToPath(new URL("../../migrations", import.meta.url));

export async function migrateDatabase(pool: Pool, migrationsRoot = defaultMigrationsRoot): Promise<void> {
  const files = (await readdir(migrationsRoot))
    .filter((file) => /^\d+_[a-z0-9_]+\.sql$/.test(file))
    .sort();
  const client = await pool.connect();
  try {
    await client.query(`CREATE TABLE IF NOT EXISTS schema_migrations (
      version varchar(255) PRIMARY KEY,
      applied_at timestamptz NOT NULL DEFAULT now()
    )`);
    for (const file of files) await applyMigration(client, migrationsRoot, file);
  } finally {
    client.release();
  }
}

async function applyMigration(client: PoolClient, root: string, file: string): Promise<void> {
  const applied = await client.query<{ exists: boolean }>(
    "SELECT EXISTS(SELECT 1 FROM schema_migrations WHERE version = $1) AS exists",
    [file],
  );
  if (applied.rows[0]?.exists) return;
  const sql = await readFile(join(root, file), "utf8");
  await client.query("BEGIN");
  try {
    await client.query(sql);
    await client.query("INSERT INTO schema_migrations(version) VALUES ($1)", [file]);
    await client.query("COMMIT");
  } catch (error) {
    await client.query("ROLLBACK");
    throw error;
  }
}
