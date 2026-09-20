export type UserRecord = {
  id: string;
  googleSubject: string;
  email: string;
  username: string;
  avatarUrl: string | null;
  disabledAt: Date | null;
  createdAt: Date;
};

export type GoogleIdentity = {
  subject: string;
  email: string;
  emailVerified: boolean;
  name: string | null;
  avatarUrl: string | null;
};

export type AuthTransactionRecord = {
  redirectUri: string;
  codeChallenge: string;
  nonce: string;
  expiresAt: Date;
};

export type MediaKind = "audio" | "image";

export type MediaMetadata = {
  kind: MediaKind;
  hash: string;
  mimeType: string;
  extension: string;
  byteSize: number;
  durationMs: number | null;
  width: number | null;
  height: number | null;
  storageKey: string;
};

export type OwnedMediaRecord = MediaMetadata & {
  id: string;
  assetId: string;
  ownerId: string;
  createdAt: Date;
};

export type PublicationRecord = {
  id: string;
  title: string;
  description: string;
  owner: UserRecord;
  audio: OwnedMediaRecord;
  image: OwnedMediaRecord | null;
  createdAt: Date;
  updatedAt: Date;
};

export type PublicationCursor = { createdAt: Date; id: string };

export type AuditEvent = {
  actorUserId: string | null;
  action: string;
  targetType: string;
  targetId: string | null;
  requestId: string | null;
  sourceIp: string | null;
  metadata?: Record<string, unknown>;
};

export interface CommunityRepository {
  health(): Promise<boolean>;
  createAuthTransaction(stateHash: Buffer, transaction: AuthTransactionRecord): Promise<void>;
  consumeAuthTransaction(stateHash: Buffer, now: Date): Promise<AuthTransactionRecord | null>;
  upsertGoogleUser(identity: GoogleIdentity): Promise<UserRecord>;
  createSession(userId: string, tokenHash: Buffer, expiresAt: Date): Promise<void>;
  authenticateSession(tokenHash: Buffer, now: Date): Promise<UserRecord | null>;
  revokeSession(tokenHash: Buffer): Promise<boolean>;
  updateUsername(userId: string, username: string): Promise<UserRecord>;
  mediaUsage(userId: string): Promise<{ bytes: number; count: number }>;
  registerOwnedMedia(userId: string, metadata: MediaMetadata): Promise<OwnedMediaRecord>;
  findOwnedMedia(userId: string, id: string, kind?: MediaKind): Promise<OwnedMediaRecord | null>;
  createPublication(userId: string, input: { title: string; description: string; audioMediaId: string; imageMediaId: string | null }): Promise<PublicationRecord>;
  listPublications(input: { query?: string; cursor?: PublicationCursor; limit: number }): Promise<PublicationRecord[]>;
  listOwnedPublications(userId: string): Promise<PublicationRecord[]>;
  findPublication(id: string, includeInactive?: boolean): Promise<PublicationRecord | null>;
  deletePublication(userId: string, id: string): Promise<boolean>;
  disablePublication(id: string, reason: string): Promise<boolean>;
  publicationCount(userId: string): Promise<number>;
  writeAudit(event: AuditEvent): Promise<void>;
  cleanupExpired(before: Date): Promise<{ authTransactions: number; sessions: number }>;
  referencedStorageKeys(): Promise<Set<string>>;
  close(): Promise<void>;
}

export class DomainError extends Error {
  constructor(
    public readonly code: "conflict" | "forbidden" | "unauthorized" | "not_found" | "quota" | "invalid",
    message: string,
  ) {
    super(message);
  }
}
