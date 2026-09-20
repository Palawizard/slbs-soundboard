import { createHash } from "node:crypto";
import { open, unlink } from "node:fs/promises";
import type { Readable } from "node:stream";
import { MAX_IMAGE_BYTES, MAX_SOUND_BYTES, type OwnedMedia } from "@slb/contracts";
import type { CommunityRepository, MediaKind, MediaMetadata, OwnedMediaRecord } from "../domain.js";
import { DomainError } from "../domain.js";
import type { MediaInspector } from "./inspector.js";
import type { ObjectStorage } from "./storage.js";

const MAX_USER_MEDIA_BYTES = 500 * 1024 * 1024;
const MAX_USER_MEDIA_COUNT = 500;

export class MediaService {
  constructor(
    private readonly repository: CommunityRepository,
    private readonly storage: ObjectStorage,
    private readonly inspector: MediaInspector,
  ) {}

  async upload(userId: string, kind: MediaKind, stream: Readable): Promise<OwnedMedia> {
    const usage = await this.repository.mediaUsage(userId);
    if (usage.count >= MAX_USER_MEDIA_COUNT) throw new DomainError("quota", "Le nombre maximal de médias est atteint.");
    const maximum = kind === "audio" ? MAX_SOUND_BYTES : MAX_IMAGE_BYTES;
    if (usage.bytes >= MAX_USER_MEDIA_BYTES) throw new DomainError("quota", "L’espace de stockage du compte est épuisé.");
    const quarantine = this.storage.quarantinePath();
    try {
      const streamed = await streamToFile(stream, quarantine, maximum);
      if (usage.bytes + streamed.byteSize > MAX_USER_MEDIA_BYTES) throw new DomainError("quota", "L’espace de stockage du compte est épuisé.");
      const inspected = kind === "audio" ? await this.inspector.inspectAudio(quarantine) : await this.inspector.inspectImage(quarantine);
      const storageKey = `${kind}/${streamed.hash.slice(0, 2)}/${streamed.hash}.${inspected.extension}`;
      const metadata: MediaMetadata = { kind, hash: streamed.hash, byteSize: streamed.byteSize, storageKey, ...inspected };
      await this.storage.promote(quarantine, storageKey);
      return ownedMedia(await this.repository.registerOwnedMedia(userId, metadata));
    } catch (error) {
      await this.storage.removeQuarantine(quarantine);
      throw error;
    }
  }
}

export async function streamToFile(stream: Readable, path: string, maximumBytes: number): Promise<{ hash: string; byteSize: number }> {
  const handle = await open(path, "wx");
  const hash = createHash("sha256");
  let byteSize = 0;
  try {
    for await (const value of stream) {
      const chunk = Buffer.isBuffer(value) ? value : Buffer.from(value as Uint8Array);
      byteSize += chunk.length;
      if (byteSize > maximumBytes) throw new DomainError("invalid", "Ce fichier dépasse la taille autorisée.");
      hash.update(chunk);
      await handle.write(chunk);
    }
    if (byteSize === 0) throw new DomainError("invalid", "Le fichier envoyé est vide.");
    await handle.sync();
    return { hash: hash.digest("hex"), byteSize };
  } catch (error) {
    await handle.close();
    await unlink(path).catch(() => undefined);
    throw error;
  } finally {
    await handle.close().catch(() => undefined);
  }
}

export function ownedMedia(media: OwnedMediaRecord): OwnedMedia {
  return {
    id: media.id,
    kind: media.kind,
    hash: media.hash,
    mimeType: media.mimeType,
    byteSize: media.byteSize,
    durationMs: media.durationMs,
    width: media.width,
    height: media.height,
    createdAt: media.createdAt.toISOString(),
  };
}
