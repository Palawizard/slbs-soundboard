import { createHmac, timingSafeEqual } from "node:crypto";
import type { CommunitySound, CreatePublicationRequest, ImportMetadata, PublicationListResponse } from "@slb/contracts";
import type { CommunityRepository, PublicationCursor, PublicationRecord } from "../domain.js";
import { DomainError } from "../domain.js";
import { ownedMedia } from "../media/service.js";
import { publicUser } from "../auth/service.js";

const MAX_ACTIVE_PUBLICATIONS = 100;

export class PublicationService {
  constructor(
    private readonly repository: CommunityRepository,
    private readonly publicBaseUrl: string,
    private readonly cursorSecret: string,
  ) {}

  async create(userId: string, input: CreatePublicationRequest): Promise<CommunitySound> {
    if (await this.repository.publicationCount(userId) >= MAX_ACTIVE_PUBLICATIONS) {
      throw new DomainError("quota", "Le nombre maximal de publications est atteint.");
    }
    return communitySound(await this.repository.createPublication(userId, input));
  }

  async list(input: { query?: string; cursor?: string; limit: number }): Promise<PublicationListResponse> {
    const cursor = input.cursor ? this.decodeCursor(input.cursor) : undefined;
    const rows = await this.repository.listPublications({
      ...(input.query ? { query: input.query } : {}),
      ...(cursor ? { cursor } : {}),
      limit: input.limit + 1,
    });
    const hasMore = rows.length > input.limit;
    const items = rows.slice(0, input.limit);
    const last = items.at(-1);
    return { items: items.map(communitySound), nextCursor: hasMore && last ? this.encodeCursor({ createdAt: last.createdAt, id: last.id }) : null };
  }

  async get(id: string): Promise<CommunitySound> {
    const publication = await this.repository.findPublication(id);
    if (!publication) throw new DomainError("not_found", "Cette publication est introuvable.");
    return communitySound(publication);
  }

  async importMetadata(id: string): Promise<ImportMetadata> {
    const publication = await this.repository.findPublication(id);
    if (!publication) throw new DomainError("not_found", "Cette publication est introuvable.");
    return {
      publication: communitySound(publication),
      audioUrl: `${this.publicBaseUrl}/v1/publications/${id}/audio`,
      imageUrl: publication.image ? `${this.publicBaseUrl}/v1/publications/${id}/image` : null,
    };
  }

  async delete(userId: string, id: string): Promise<void> {
    if (!(await this.repository.deletePublication(userId, id))) throw new DomainError("not_found", "Cette publication est introuvable.");
  }

  private encodeCursor(cursor: PublicationCursor): string {
    const payload = Buffer.from(JSON.stringify({ createdAt: cursor.createdAt.toISOString(), id: cursor.id })).toString("base64url");
    const signature = createHmac("sha256", this.cursorSecret).update(payload).digest("base64url");
    return `${payload}.${signature}`;
  }

  private decodeCursor(cursor: string): PublicationCursor {
    const [payload, signature, extra] = cursor.split(".");
    if (!payload || !signature || extra) throw new DomainError("invalid", "Le curseur de pagination est invalide.");
    const expected = createHmac("sha256", this.cursorSecret).update(payload).digest("base64url");
    const left = Buffer.from(signature); const right = Buffer.from(expected);
    if (left.length !== right.length || !timingSafeEqual(left, right)) throw new DomainError("invalid", "Le curseur de pagination est invalide.");
    try {
      const value = JSON.parse(Buffer.from(payload, "base64url").toString("utf8")) as { createdAt?: unknown; id?: unknown };
      const createdAt = new Date(String(value.createdAt));
      if (!Number.isFinite(createdAt.getTime()) || typeof value.id !== "string" || !/^[0-9a-f-]{36}$/i.test(value.id)) throw new Error();
      return { createdAt, id: value.id };
    } catch { throw new DomainError("invalid", "Le curseur de pagination est invalide."); }
  }
}

export function communitySound(publication: PublicationRecord): CommunitySound {
  return {
    id: publication.id,
    title: publication.title,
    description: publication.description,
    owner: publicUser(publication.owner),
    audio: ownedMedia(publication.audio),
    image: publication.image ? ownedMedia(publication.image) : null,
    createdAt: publication.createdAt.toISOString(),
    updatedAt: publication.updatedAt.toISOString(),
  };
}
