import { randomUUID } from "node:crypto";
import { Pool, type PoolClient, type QueryResultRow } from "pg";
import type {
  AuditEvent,
  AuthTransactionRecord,
  CommunityRepository,
  GoogleIdentity,
  MediaKind,
  MediaMetadata,
  OwnedMediaRecord,
  PublicationCursor,
  PublicationRecord,
  UserRecord,
} from "../domain.js";
import { DomainError } from "../domain.js";

type DatabaseRow = QueryResultRow & Record<string, unknown>;

export class PostgresCommunityRepository implements CommunityRepository {
  readonly pool: Pool;

  constructor(connectionString: string) {
    this.pool = new Pool({ connectionString, max: 20, idleTimeoutMillis: 30_000, connectionTimeoutMillis: 5_000 });
  }

  async health(): Promise<boolean> {
    try { await this.pool.query("SELECT 1"); return true; } catch { return false; }
  }

  async createAuthTransaction(stateHash: Buffer, transaction: AuthTransactionRecord): Promise<void> {
    await this.pool.query(
      `INSERT INTO oauth_transactions(state_hash, redirect_uri, code_challenge, nonce, expires_at)
       VALUES ($1, $2, $3, $4, $5)`,
      [stateHash, transaction.redirectUri, transaction.codeChallenge, transaction.nonce, transaction.expiresAt],
    );
  }

  async consumeAuthTransaction(stateHash: Buffer, now: Date): Promise<AuthTransactionRecord | null> {
    const result = await this.pool.query<DatabaseRow>(
      `UPDATE oauth_transactions SET consumed_at = $2
       WHERE state_hash = $1 AND consumed_at IS NULL AND expires_at > $2
       RETURNING redirect_uri, code_challenge, nonce, expires_at`,
      [stateHash, now],
    );
    const row = result.rows[0];
    return row ? { redirectUri: string(row.redirect_uri), codeChallenge: string(row.code_challenge), nonce: string(row.nonce), expiresAt: date(row.expires_at) } : null;
  }

  async upsertGoogleUser(identity: GoogleIdentity): Promise<UserRecord> {
    const client = await this.pool.connect();
    try {
      await client.query("BEGIN");
      const existing = await client.query<DatabaseRow>("SELECT * FROM users WHERE google_subject = $1 FOR UPDATE", [identity.subject]);
      let row: DatabaseRow;
      if (existing.rows[0]) {
        const updated = await client.query<DatabaseRow>(
          `UPDATE users SET email = $2, avatar_url = $3, updated_at = now()
           WHERE google_subject = $1 RETURNING *`,
          [identity.subject, identity.email, identity.avatarUrl],
        );
        row = required(updated.rows[0]);
      } else {
        const username = await availableUsername(client, identity.name ?? "Utilisateur", identity.subject);
        const inserted = await client.query<DatabaseRow>(
          `INSERT INTO users(id, google_subject, email, username, username_key, avatar_url)
           VALUES ($1, $2, $3, $4, lower($4), $5) RETURNING *`,
          [randomUUID(), identity.subject, identity.email, username, identity.avatarUrl],
        );
        row = required(inserted.rows[0]);
      }
      await client.query("COMMIT");
      return mapUser(row);
    } catch (error) {
      await client.query("ROLLBACK");
      throw error;
    } finally { client.release(); }
  }

  async createSession(userId: string, tokenHash: Buffer, expiresAt: Date): Promise<void> {
    await this.pool.query("INSERT INTO sessions(id, user_id, token_hash, expires_at) VALUES ($1, $2, $3, $4)", [randomUUID(), userId, tokenHash, expiresAt]);
  }

  async authenticateSession(tokenHash: Buffer, now: Date): Promise<UserRecord | null> {
    const result = await this.pool.query<DatabaseRow>(
      `UPDATE sessions s SET last_used_at = $2 FROM users u
       WHERE s.token_hash = $1 AND s.user_id = u.id AND s.revoked_at IS NULL
         AND s.expires_at > $2 AND u.disabled_at IS NULL
       RETURNING u.*`,
      [tokenHash, now],
    );
    return result.rows[0] ? mapUser(result.rows[0]) : null;
  }

  async revokeSession(tokenHash: Buffer): Promise<boolean> {
    return (await this.pool.query("UPDATE sessions SET revoked_at = now() WHERE token_hash = $1 AND revoked_at IS NULL", [tokenHash])).rowCount === 1;
  }

  async updateUsername(userId: string, username: string): Promise<UserRecord> {
    try {
      const result = await this.pool.query<DatabaseRow>(
        "UPDATE users SET username = $2, username_key = lower($2), updated_at = now() WHERE id = $1 AND disabled_at IS NULL RETURNING *",
        [userId, username],
      );
      if (!result.rows[0]) throw new DomainError("not_found", "Ce compte est introuvable.");
      return mapUser(result.rows[0]);
    } catch (error) {
      if (isPgCode(error, "23505")) throw new DomainError("conflict", "Ce pseudonyme est déjà utilisé.");
      throw error;
    }
  }

  async mediaUsage(userId: string): Promise<{ bytes: number; count: number }> {
    const result = await this.pool.query<{ bytes: string; count: string }>(
      `SELECT COALESCE(sum(a.byte_size), 0)::text AS bytes, count(*)::text AS count
       FROM user_media um JOIN media_assets a ON a.id = um.asset_id
       WHERE um.user_id = $1 AND um.deleted_at IS NULL`, [userId],
    );
    return { bytes: Number(result.rows[0]?.bytes ?? 0), count: Number(result.rows[0]?.count ?? 0) };
  }

  async registerOwnedMedia(userId: string, metadata: MediaMetadata): Promise<OwnedMediaRecord> {
    const client = await this.pool.connect();
    try {
      await client.query("BEGIN");
      const asset = await client.query<DatabaseRow>(
        `INSERT INTO media_assets(id, kind, sha256, mime_type, extension, byte_size, duration_ms, width, height, storage_key)
         VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10)
         ON CONFLICT (kind, sha256) DO UPDATE SET sha256 = EXCLUDED.sha256 RETURNING *`,
        [randomUUID(), metadata.kind, metadata.hash, metadata.mimeType, metadata.extension, metadata.byteSize, metadata.durationMs, metadata.width, metadata.height, metadata.storageKey],
      );
      const owned = await client.query<DatabaseRow>(
        `INSERT INTO user_media(id, user_id, asset_id) VALUES ($1,$2,$3)
         ON CONFLICT (user_id, asset_id) DO UPDATE SET deleted_at = NULL
         RETURNING id, user_id, created_at`,
        [randomUUID(), userId, required(asset.rows[0]).id],
      );
      await client.query("COMMIT");
      return mapOwnedMedia({ ...required(asset.rows[0]), ...required(owned.rows[0]), asset_id: required(asset.rows[0]).id });
    } catch (error) { await client.query("ROLLBACK"); throw error; } finally { client.release(); }
  }

  async findOwnedMedia(userId: string, id: string, kind?: MediaKind): Promise<OwnedMediaRecord | null> {
    const values: unknown[] = [userId, id];
    const result = await this.pool.query<DatabaseRow>(
      `${ownedMediaSelect} WHERE um.user_id = $1 AND um.id = $2 AND um.deleted_at IS NULL${kind ? " AND a.kind = $3" : ""}`,
      kind ? [...values, kind] : values,
    );
    return result.rows[0] ? mapOwnedMedia(result.rows[0]) : null;
  }

  async createPublication(userId: string, input: { title: string; description: string; audioMediaId: string; imageMediaId: string | null }): Promise<PublicationRecord> {
    if (!(await this.findOwnedMedia(userId, input.audioMediaId, "audio"))) throw new DomainError("forbidden", "Ce fichier audio ne vous appartient pas.");
    if (input.imageMediaId && !(await this.findOwnedMedia(userId, input.imageMediaId, "image"))) throw new DomainError("forbidden", "Cette image ne vous appartient pas.");
    const id = randomUUID();
    await this.pool.query(
      `INSERT INTO publications(id, owner_id, audio_media_id, image_media_id, title, description)
       VALUES ($1,$2,$3,$4,$5,$6)`,
      [id, userId, input.audioMediaId, input.imageMediaId, input.title, input.description],
    );
    return required(await this.findPublication(id, true));
  }

  async listPublications(input: { query?: string; cursor?: PublicationCursor; limit: number }): Promise<PublicationRecord[]> {
    const values: unknown[] = [];
    const conditions = ["p.status = 'active'", "u.disabled_at IS NULL"];
    if (input.query) { values.push(input.query); conditions.push(`to_tsvector('simple', p.title || ' ' || p.description) @@ plainto_tsquery('simple', $${values.length})`); }
    if (input.cursor) {
      values.push(input.cursor.createdAt, input.cursor.id);
      conditions.push(`(p.created_at, p.id) < ($${values.length - 1}, $${values.length})`);
    }
    values.push(input.limit);
    const result = await this.pool.query<DatabaseRow>(
      `${publicationSelect} WHERE ${conditions.join(" AND ")} ORDER BY p.created_at DESC, p.id DESC LIMIT $${values.length}`,
      values,
    );
    return result.rows.map(mapPublication);
  }

  async findPublication(id: string, includeInactive = false): Promise<PublicationRecord | null> {
    const result = await this.pool.query<DatabaseRow>(
      `${publicationSelect} WHERE p.id = $1 ${includeInactive ? "" : "AND p.status = 'active' AND u.disabled_at IS NULL"}`,
      [id],
    );
    return result.rows[0] ? mapPublication(result.rows[0]) : null;
  }

  async deletePublication(userId: string, id: string): Promise<boolean> {
    return (await this.pool.query(
      "UPDATE publications SET status = 'deleted', deleted_at = now(), updated_at = now() WHERE id = $1 AND owner_id = $2 AND status <> 'deleted'",
      [id, userId],
    )).rowCount === 1;
  }

  async disablePublication(id: string, reason: string): Promise<boolean> {
    return (await this.pool.query(
      "UPDATE publications SET status = 'disabled', disabled_reason = $2, updated_at = now() WHERE id = $1 AND status = 'active'",
      [id, reason],
    )).rowCount === 1;
  }

  async publicationCount(userId: string): Promise<number> {
    const result = await this.pool.query<{ count: string }>("SELECT count(*)::text AS count FROM publications WHERE owner_id = $1 AND status = 'active'", [userId]);
    return Number(result.rows[0]?.count ?? 0);
  }

  async writeAudit(event: AuditEvent): Promise<void> {
    await this.pool.query(
      `INSERT INTO audit_events(actor_user_id, action, target_type, target_id, request_id, source_ip, metadata)
       VALUES ($1,$2,$3,$4,$5,$6,$7)`,
      [event.actorUserId, event.action, event.targetType, event.targetId, event.requestId, event.sourceIp, event.metadata ?? {}],
    );
  }

  async cleanupExpired(before: Date): Promise<{ authTransactions: number; sessions: number }> {
    const auth = await this.pool.query("DELETE FROM oauth_transactions WHERE expires_at < $1 OR consumed_at < $1", [before]);
    const sessions = await this.pool.query("DELETE FROM sessions WHERE expires_at < $1 OR revoked_at < $1", [before]);
    return { authTransactions: auth.rowCount ?? 0, sessions: sessions.rowCount ?? 0 };
  }

  async close(): Promise<void> { await this.pool.end(); }
}

const ownedMediaSelect = `SELECT um.id, um.user_id, um.asset_id, um.created_at,
  a.kind, a.sha256, a.mime_type, a.extension, a.byte_size, a.duration_ms, a.width, a.height, a.storage_key
  FROM user_media um JOIN media_assets a ON a.id = um.asset_id`;

const publicationSelect = `SELECT p.id AS publication_id, p.title, p.description, p.created_at AS publication_created_at, p.updated_at,
  u.id AS owner_id, u.google_subject, u.email, u.username, u.avatar_url, u.disabled_at, u.created_at AS user_created_at,
  am.id AS audio_id, aa.id AS audio_asset_id, aa.kind AS audio_kind, aa.sha256 AS audio_sha256, aa.mime_type AS audio_mime_type,
  aa.extension AS audio_extension, aa.byte_size AS audio_byte_size, aa.duration_ms AS audio_duration_ms,
  aa.width AS audio_width, aa.height AS audio_height, aa.storage_key AS audio_storage_key, am.created_at AS audio_created_at,
  im.id AS image_id, ia.id AS image_asset_id, ia.kind AS image_kind, ia.sha256 AS image_sha256, ia.mime_type AS image_mime_type,
  ia.extension AS image_extension, ia.byte_size AS image_byte_size, ia.duration_ms AS image_duration_ms,
  ia.width AS image_width, ia.height AS image_height, ia.storage_key AS image_storage_key, im.created_at AS image_created_at
  FROM publications p JOIN users u ON u.id = p.owner_id
  JOIN user_media am ON am.id = p.audio_media_id JOIN media_assets aa ON aa.id = am.asset_id
  LEFT JOIN user_media im ON im.id = p.image_media_id LEFT JOIN media_assets ia ON ia.id = im.asset_id`;

async function availableUsername(client: PoolClient, name: string, subject: string): Promise<string> {
  const base = name.normalize("NFKD").replace(/[^A-Za-z0-9_]/g, "").slice(0, 24) || "Utilisateur";
  const safeBase = base.length >= 3 ? base : `${base}SLB`;
  for (let attempt = 0; attempt < 20; attempt += 1) {
    const suffix = attempt === 0 ? "" : `_${subject.slice(-4)}${attempt === 1 ? "" : attempt}`;
    const candidate = `${safeBase.slice(0, 32 - suffix.length)}${suffix}`;
    const result = await client.query("SELECT 1 FROM users WHERE username_key = lower($1)", [candidate]);
    if (result.rowCount === 0) return candidate;
  }
  return `Utilisateur_${randomUUID().slice(0, 8)}`;
}

function mapUser(row: DatabaseRow): UserRecord {
  return { id: string(row.id ?? row.owner_id), googleSubject: string(row.google_subject), email: string(row.email), username: string(row.username), avatarUrl: nullableString(row.avatar_url), disabledAt: nullableDate(row.disabled_at), createdAt: date(row.created_at ?? row.user_created_at) };
}

function mapOwnedMedia(row: DatabaseRow, prefix = ""): OwnedMediaRecord {
  const key = (name: string) => row[`${prefix}${name}`];
  return { id: string(key("id")), assetId: string(key("asset_id")), ownerId: string(row.user_id ?? row.owner_id), kind: string(key("kind")) as MediaKind, hash: string(key("sha256")), mimeType: string(key("mime_type")), extension: string(key("extension")), byteSize: Number(key("byte_size")), durationMs: nullableNumber(key("duration_ms")), width: nullableNumber(key("width")), height: nullableNumber(key("height")), storageKey: string(key("storage_key")), createdAt: date(key("created_at")) };
}

function mapPublication(row: DatabaseRow): PublicationRecord {
  return { id: string(row.publication_id), title: string(row.title), description: string(row.description), owner: mapUser(row), audio: mapOwnedMedia(row, "audio_"), image: row.image_id ? mapOwnedMedia(row, "image_") : null, createdAt: date(row.publication_created_at), updatedAt: date(row.updated_at) };
}

function required<T>(value: T | null | undefined): T { if (value == null) throw new Error("Database returned no row"); return value; }
function string(value: unknown): string { return String(value); }
function date(value: unknown): Date { return value instanceof Date ? value : new Date(String(value)); }
function nullableDate(value: unknown): Date | null { return value == null ? null : date(value); }
function nullableString(value: unknown): string | null { return value == null ? null : String(value); }
function nullableNumber(value: unknown): number | null { return value == null ? null : Number(value); }
function isPgCode(error: unknown, code: string): boolean { return typeof error === "object" && error !== null && "code" in error && error.code === code; }
