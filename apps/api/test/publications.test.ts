import { describe, expect, it, vi } from "vitest";
import { PublicationService } from "../src/publications/service.js";
import { parseByteRange, RangeNotSatisfiableError } from "../src/publications/range.js";
import type { CommunityRepository, PublicationRecord } from "../src/domain.js";

const owner = { id: "123e4567-e89b-12d3-a456-426614174000", googleSubject: "g", email: "u@example.com", username: "User_1", avatarUrl: null, disabledAt: null, createdAt: new Date("2026-01-01T00:00:00Z") };
function publication(id: string, createdAt: string): PublicationRecord {
  const media = { id: "223e4567-e89b-12d3-a456-426614174000", assetId: "323e4567-e89b-12d3-a456-426614174000", ownerId: owner.id, kind: "audio" as const, hash: "a".repeat(64), mimeType: "audio/wav", extension: "wav", byteSize: 100, durationMs: 1000, width: null, height: null, storageKey: `audio/aa/${"a".repeat(64)}.wav`, createdAt: new Date(createdAt) };
  return { id, title: "Cloche", description: "", owner, audio: media, image: null, createdAt: new Date(createdAt), updatedAt: new Date(createdAt) };
}

describe("publication pagination", () => {
  it("signs cursors and rejects tampering", async () => {
    const rows = [publication("423e4567-e89b-12d3-a456-426614174001", "2026-01-03T00:00:00Z"), publication("423e4567-e89b-12d3-a456-426614174002", "2026-01-02T00:00:00Z"), publication("423e4567-e89b-12d3-a456-426614174003", "2026-01-01T00:00:00Z")];
    const repository = { listPublications: vi.fn(async () => rows) } as unknown as CommunityRepository;
    const service = new PublicationService(repository, "https://community.example.test", "s".repeat(32));
    const first = await service.list({ limit: 2 });
    expect(first.items).toHaveLength(2); expect(first.nextCursor).toContain(".");
    await service.list({ limit: 2, cursor: first.nextCursor! });
    expect(repository.listPublications).toHaveBeenLastCalledWith(expect.objectContaining({ cursor: { createdAt: new Date("2026-01-02T00:00:00Z"), id: rows[1]!.id } }));
    await expect(service.list({ limit: 2, cursor: `${first.nextCursor}x` })).rejects.toThrow("curseur");
  });
});

describe("HTTP byte ranges", () => {
  it("parses bounded, open and suffix ranges", () => {
    expect(parseByteRange("bytes=10-19", 100)).toEqual({ start: 10, end: 19, length: 10 });
    expect(parseByteRange("bytes=90-", 100)).toEqual({ start: 90, end: 99, length: 10 });
    expect(parseByteRange("bytes=-8", 100)).toEqual({ start: 92, end: 99, length: 8 });
  });

  it("rejects multiple and unsatisfiable ranges", () => {
    expect(() => parseByteRange("bytes=100-200", 100)).toThrow(RangeNotSatisfiableError);
    expect(() => parseByteRange("bytes=0-1,4-5", 100)).toThrow(RangeNotSatisfiableError);
  });
});
