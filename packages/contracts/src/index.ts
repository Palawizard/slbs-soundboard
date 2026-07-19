import { z } from "zod";

export const API_VERSION = "v1" as const;
export const MAX_SOUND_BYTES = 25 * 1024 * 1024;
export const MAX_IMAGE_BYTES = 5 * 1024 * 1024;
export const MAX_SOUND_DURATION_MS = 10 * 60 * 1000;

export const healthResponseSchema = z.object({
  status: z.literal("ok"),
  service: z.literal("community-api"),
  database: z.enum(["ready", "unavailable"]).optional(),
});

export const apiErrorSchema = z.object({
  error: z.object({
    code: z.string().min(1).max(64),
    message: z.string().min(1).max(240),
    requestId: z.string().min(1).max(128),
  }),
});

export const idSchema = z.string().uuid();
export const usernameSchema = z.string().trim().min(3).max(32).regex(/^[A-Za-z0-9_]+$/);
export const sha256Schema = z.string().regex(/^[a-f0-9]{64}$/);
export const isoDateSchema = z.string().datetime({ offset: true });

export const publicUserSchema = z.object({
  id: idSchema,
  username: usernameSchema,
  avatarUrl: z.string().url().nullable(),
  createdAt: isoDateSchema,
});

export const authStartRequestSchema = z.object({
  redirectUri: z.string().url().max(512),
  codeChallenge: z.string().regex(/^[A-Za-z0-9_-]{43,128}$/),
});

export const authStartResponseSchema = z.object({
  authorizationUrl: z.string().url(),
  state: z.string().regex(/^[A-Za-z0-9_-]{32,128}$/),
  expiresAt: isoDateSchema,
});

export const authExchangeRequestSchema = z.object({
  state: z.string().regex(/^[A-Za-z0-9_-]{32,128}$/),
  code: z.string().min(8).max(4096),
  codeVerifier: z.string().regex(/^[A-Za-z0-9._~-]{43,128}$/),
  redirectUri: z.string().url().max(512),
});

export const sessionResponseSchema = z.object({
  token: z.string().regex(/^[A-Za-z0-9_-]{43,128}$/),
  expiresAt: isoDateSchema,
  user: publicUserSchema,
});

export const updateUsernameRequestSchema = z.object({ username: usernameSchema });

export const mediaKindSchema = z.enum(["audio", "image"]);
export const ownedMediaSchema = z.object({
  id: idSchema,
  kind: mediaKindSchema,
  hash: sha256Schema,
  mimeType: z.string().min(3).max(100),
  byteSize: z.number().int().positive(),
  durationMs: z.number().int().positive().nullable(),
  width: z.number().int().positive().nullable(),
  height: z.number().int().positive().nullable(),
  createdAt: isoDateSchema,
});

export const createPublicationRequestSchema = z.object({
  title: z.string().trim().min(1).max(120),
  description: z.string().trim().max(500).default(""),
  audioMediaId: idSchema,
  imageMediaId: idSchema.nullable().default(null),
});

export const communitySoundSchema = z.object({
  id: idSchema,
  title: z.string().min(1).max(120),
  description: z.string().max(500),
  owner: publicUserSchema,
  audio: ownedMediaSchema,
  image: ownedMediaSchema.nullable(),
  createdAt: isoDateSchema,
  updatedAt: isoDateSchema,
});

export const publicationListQuerySchema = z.object({
  q: z.string().trim().max(80).optional(),
  cursor: z.string().regex(/^[A-Za-z0-9_.-]{8,512}$/).optional(),
  limit: z.coerce.number().int().min(1).max(50).default(20),
});

export const publicationListResponseSchema = z.object({
  items: z.array(communitySoundSchema),
  nextCursor: z.string().nullable(),
});

export const importMetadataSchema = z.object({
  publication: communitySoundSchema,
  audioUrl: z.string().url(),
  imageUrl: z.string().url().nullable(),
});

export type HealthResponse = z.infer<typeof healthResponseSchema>;
export type ApiError = z.infer<typeof apiErrorSchema>;
export type PublicUser = z.infer<typeof publicUserSchema>;
export type AuthStartRequest = z.infer<typeof authStartRequestSchema>;
export type AuthStartResponse = z.infer<typeof authStartResponseSchema>;
export type AuthExchangeRequest = z.infer<typeof authExchangeRequestSchema>;
export type SessionResponse = z.infer<typeof sessionResponseSchema>;
export type OwnedMedia = z.infer<typeof ownedMediaSchema>;
export type CreatePublicationRequest = z.infer<typeof createPublicationRequestSchema>;
export type CommunitySound = z.infer<typeof communitySoundSchema>;
export type PublicationListQuery = z.infer<typeof publicationListQuerySchema>;
export type PublicationListResponse = z.infer<typeof publicationListResponseSchema>;
export type ImportMetadata = z.infer<typeof importMetadataSchema>;
