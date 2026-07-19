import { Readable } from "node:stream";
import { mkdtemp, readdir, readFile, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { afterEach, describe, expect, it } from "vitest";
import { MAX_IMAGE_BYTES } from "@slb/contracts";
import { streamToFile } from "../src/media/service.js";
import { LocalObjectStorage } from "../src/media/storage.js";

const roots: string[] = [];
afterEach(async () => { await Promise.all(roots.splice(0).map((root) => rm(root, { recursive: true, force: true }))); });

describe("quarantine media storage", () => {
  it("hashes a bounded stream without loading it as one buffer", async () => {
    const root = await mkdtemp(join(tmpdir(), "slb-media-")); roots.push(root);
    const path = join(root, "upload");
    const result = await streamToFile(Readable.from([Buffer.from("abc"), Buffer.from("def")]), path, 8);
    expect(result).toEqual({ hash: "bef57ec7f53a6d40beb640a780a639c83bc29ac8a9816f1fc6c5c6dcd93c4721", byteSize: 6 });
    expect(await readFile(path, "utf8")).toBe("abcdef");
  });

  it("rejects oversized input at the byte boundary", async () => {
    const root = await mkdtemp(join(tmpdir(), "slb-media-")); roots.push(root);
    await expect(streamToFile(Readable.from([Buffer.alloc(MAX_IMAGE_BYTES + 1)]), join(root, "upload"), MAX_IMAGE_BYTES)).rejects.toThrow("taille");
  });

  it("promotes only validated storage keys and cleans old quarantine files", async () => {
    const root = await mkdtemp(join(tmpdir(), "slb-media-")); roots.push(root);
    const storage = new LocalObjectStorage(root); await storage.initialize();
    const source = storage.quarantinePath();
    await streamToFile(Readable.from([Buffer.from("data")]), source, 10);
    const hash = "a".repeat(64); const key = `audio/aa/${hash}.wav`;
    await storage.promote(source, key);
    expect(await readFile(storage.resolve(key), "utf8")).toBe("data");
    expect(() => storage.resolve("../../secret")).toThrow();
    expect(await readdir(join(root, "quarantine"))).toHaveLength(0);
  });
});
