import { createReadStream } from "node:fs";
import Fastify, { type FastifyInstance, type FastifyReply, type FastifyRequest } from "fastify";
import multipart from "@fastify/multipart";
import rateLimit from "@fastify/rate-limit";
import { ZodError } from "zod";
import {
  API_VERSION,
  authExchangeRequestSchema,
  authStartRequestSchema,
  createPublicationRequestSchema,
  healthResponseSchema,
  idSchema,
  MAX_IMAGE_BYTES,
  MAX_SOUND_BYTES,
  publicationListQuerySchema,
  updateUsernameRequestSchema,
} from "@slb/contracts";
import type { ApiConfig } from "./config.js";
import type { CommunityRepository, MediaKind, PublicationRecord, UserRecord } from "./domain.js";
import { DomainError } from "./domain.js";
import type { AuthService } from "./auth/service.js";
import { publicUser } from "./auth/service.js";
import type { MediaService } from "./media/service.js";
import type { ObjectStorage } from "./media/storage.js";
import type { PublicationService } from "./publications/service.js";
import { parseByteRange, RangeNotSatisfiableError } from "./publications/range.js";
import { secureEqual } from "./security.js";

export type ServerDependencies = {
  config: ApiConfig;
  repository: CommunityRepository;
  auth: AuthService;
  media: MediaService;
  publications: PublicationService;
  storage: ObjectStorage;
};

export function buildServer(dependencies: ServerDependencies): FastifyInstance {
  const { config, repository, auth, media, publications, storage } = dependencies;
  const server = Fastify({ logger: config.environment !== "test", trustProxy: config.trustProxy, bodyLimit: MAX_SOUND_BYTES + 1024 * 1024, exposeHeadRoutes: false });
  server.register(rateLimit, { global: true, max: 120, timeWindow: "1 minute" });
  server.register(multipart, { limits: { files: 1, fields: 0, parts: 1, fileSize: MAX_SOUND_BYTES + 1 } });

  server.get("/health", async (_request, reply) => {
    const database = await repository.health();
    const payload = healthResponseSchema.parse({ status: "ok", service: "community-api", database: database ? "ready" : "unavailable" });
    return reply.code(database ? 200 : 503).send(payload);
  });

  server.post(`/v1/auth/google/start`, { config: { rateLimit: { max: 10, timeWindow: "1 minute" } } }, async (request, reply) => {
    const result = await auth.start(authStartRequestSchema.parse(request.body));
    await audit(repository, request, null, "auth.start", "oauth", null);
    return reply.code(201).send(result);
  });
  server.post(`/v1/auth/google/exchange`, { config: { rateLimit: { max: 10, timeWindow: "1 minute" } } }, async (request, reply) => {
    const session = await auth.exchange(authExchangeRequestSchema.parse(request.body));
    await audit(repository, request, session.user.id, "auth.login", "user", session.user.id);
    return reply.send(session);
  });
  server.get(`/v1/me`, async (request) => publicUser(await auth.authenticate(request.headers.authorization)));
  server.patch(`/v1/me`, async (request) => {
    const user = await auth.authenticate(request.headers.authorization);
    const input = updateUsernameRequestSchema.parse(request.body);
    const updated = await repository.updateUsername(user.id, input.username);
    await audit(repository, request, user.id, "user.rename", "user", user.id, { username: input.username });
    return publicUser(updated);
  });
  server.delete(`/v1/session`, async (request, reply) => {
    const user = await auth.authenticate(request.headers.authorization);
    await auth.logout(request.headers.authorization);
    await audit(repository, request, user.id, "auth.logout", "session", null);
    return reply.code(204).send();
  });

  for (const kind of ["audio", "image"] as const) {
    server.post(`/v1/media/${kind}`, { config: { rateLimit: { max: 20, timeWindow: "1 hour" } } }, async (request, reply) => {
      const user = await auth.authenticate(request.headers.authorization);
      const part = await request.file({ limits: { files: 1, fields: 0, parts: 1, fileSize: (kind === "audio" ? MAX_SOUND_BYTES : MAX_IMAGE_BYTES) + 1 } });
      if (!part) throw new DomainError("invalid", "Aucun fichier n’a été envoyé.");
      const uploaded = await media.upload(user.id, kind, part.file);
      if (part.file.truncated) throw new DomainError("invalid", "Ce fichier dépasse la taille autorisée.");
      await audit(repository, request, user.id, "media.upload", kind, uploaded.id, { hash: uploaded.hash, byteSize: uploaded.byteSize });
      return reply.code(201).send(uploaded);
    });
  }

  server.post(`/v1/publications`, async (request, reply) => {
    const user = await auth.authenticate(request.headers.authorization);
    const publication = await publications.create(user.id, createPublicationRequestSchema.parse(request.body));
    await audit(repository, request, user.id, "publication.create", "publication", publication.id);
    return reply.code(201).send(publication);
  });
  server.get(`/v1/publications`, async (request) => {
    const query = publicationListQuerySchema.parse(request.query);
    return publications.list({ ...(query.q ? { query: query.q } : {}), ...(query.cursor ? { cursor: query.cursor } : {}), limit: query.limit });
  });
  server.get(`/v1/me/publications`, async (request) => {
    const user = await auth.authenticate(request.headers.authorization);
    return { items: await publications.listOwned(user.id) };
  });
  server.get<{ Params: { id: string } }>(`/v1/publications/:id`, async (request) => publications.get(idSchema.parse(request.params.id)));
  server.get<{ Params: { id: string } }>(`/v1/publications/:id/import`, async (request) => {
    await auth.authenticate(request.headers.authorization);
    return publications.importMetadata(idSchema.parse(request.params.id));
  });
  server.delete<{ Params: { id: string } }>(`/v1/publications/:id`, async (request, reply) => {
    const user = await auth.authenticate(request.headers.authorization); const id = idSchema.parse(request.params.id);
    await publications.delete(user.id, id); await audit(repository, request, user.id, "publication.delete", "publication", id);
    return reply.code(204).send();
  });
  for (const kind of ["audio", "image"] as const) {
    server.get<{ Params: { id: string } }>(`/v1/publications/:id/${kind}`, async (request, reply) => streamPublicationMedia(request, reply, repository, storage, kind));
    server.head<{ Params: { id: string } }>(`/v1/publications/:id/${kind}`, async (request, reply) => streamPublicationMedia(request, reply, repository, storage, kind, true));
  }

  server.post<{ Params: { id: string } }>(`/v1/admin/publications/:id/disable`, async (request, reply) => {
    const configured = config.adminToken;
    const provided = typeof request.headers["x-admin-token"] === "string" ? request.headers["x-admin-token"] : "";
    if (!configured || !secureEqual(configured, provided)) throw new DomainError("forbidden", "Cette opération n’est pas autorisée.");
    const body = request.body as { reason?: unknown }; const reason = typeof body?.reason === "string" ? body.reason.trim().slice(0, 240) : "";
    if (!reason || !(await repository.disablePublication(idSchema.parse(request.params.id), reason))) throw new DomainError("not_found", "Cette publication est introuvable.");
    await audit(repository, request, null, "publication.disable", "publication", request.params.id, { reason });
    return reply.code(204).send();
  });

  server.setErrorHandler((error, request, reply) => {
    if (error instanceof RangeNotSatisfiableError) return reply.code(416).header("content-range", `bytes */${error.size}`).send(apiError("invalid_range", "La plage demandée n’est pas disponible.", request.id));
    if (error instanceof ZodError) return reply.code(400).send(apiError("invalid_request", "La requête n’est pas valide.", request.id));
    if (error instanceof DomainError) {
      const status = { conflict: 409, forbidden: 403, unauthorized: 401, not_found: 404, quota: 429, invalid: 422 }[error.code];
      return reply.code(status).send(apiError(error.code, error.message, request.id));
    }
    if (typeof error === "object" && error !== null && "statusCode" in error && typeof error.statusCode === "number" && error.statusCode < 500) return reply.code(error.statusCode).send(apiError("invalid_upload", "Le fichier envoyé n’est pas valide.", request.id));
    request.log.error(error);
    return reply.code(500).send(apiError("internal_error", "Le service est momentanément indisponible.", request.id));
  });
  return server;
}

async function streamPublicationMedia(
  request: FastifyRequest<{ Params: { id: string } }>, reply: FastifyReply, repository: CommunityRepository,
  storage: ObjectStorage, kind: MediaKind, headOnly = false,
) {
  const publication = await repository.findPublication(idSchema.parse(request.params.id));
  if (!publication) throw new DomainError("not_found", "Cette publication est introuvable.");
  const media = kind === "audio" ? publication.audio : publication.image;
  if (!media) throw new DomainError("not_found", "Ce média est introuvable.");
  const etag = `"${media.hash}"`;
  if (request.headers["if-none-match"] === etag) return reply.code(304).send();
  reply.header("accept-ranges", "bytes").header("cache-control", "public, max-age=31536000, immutable").header("etag", etag).type(media.mimeType);
  let range;
  try { range = parseByteRange(request.headers.range, media.byteSize); }
  catch (error) { if (error instanceof RangeNotSatisfiableError) reply.header("content-range", `bytes */${media.byteSize}`); throw error; }
  if (!range) {
    reply.header("content-length", media.byteSize);
    return headOnly ? reply.send() : reply.send(createReadStream(storage.resolve(media.storageKey)));
  }
  reply.code(206).header("content-length", range.length).header("content-range", `bytes ${range.start}-${range.end}/${media.byteSize}`);
  return headOnly ? reply.send() : reply.send(createReadStream(storage.resolve(media.storageKey), { start: range.start, end: range.end }));
}

async function audit(repository: CommunityRepository, request: FastifyRequest, actorUserId: string | null, action: string, targetType: string, targetId: string | null, metadata?: Record<string, unknown>) {
  await repository.writeAudit({ actorUserId, action, targetType, targetId, requestId: request.id, sourceIp: request.ip, ...(metadata ? { metadata } : {}) });
}

function apiError(code: string, message: string, requestId: string) { return { error: { code, message, requestId } }; }
