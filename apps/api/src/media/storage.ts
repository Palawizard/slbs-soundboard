import { randomUUID } from "node:crypto";
import { access, mkdir, readdir, rename, stat, unlink } from "node:fs/promises";
import { dirname, join, relative, resolve } from "node:path";

export interface ObjectStorage {
  initialize(): Promise<void>;
  quarantinePath(): string;
  promote(quarantinePath: string, storageKey: string): Promise<void>;
  resolve(storageKey: string): string;
  removeQuarantine(path: string): Promise<void>;
  cleanupQuarantine(olderThan: Date): Promise<number>;
  cleanupOrphans(referencedKeys: Set<string>, olderThan: Date): Promise<number>;
}

export class LocalObjectStorage implements ObjectStorage {
  private readonly quarantineRoot: string;
  private readonly objectRoot: string;

  constructor(private readonly root: string) {
    this.quarantineRoot = join(root, "quarantine");
    this.objectRoot = join(root, "objects");
  }

  async initialize(): Promise<void> {
    await Promise.all([
      mkdir(this.quarantineRoot, { recursive: true }),
      mkdir(this.objectRoot, { recursive: true }),
    ]);
  }

  quarantinePath(): string {
    return join(this.quarantineRoot, `${randomUUID()}.upload`);
  }

  async promote(quarantinePath: string, storageKey: string): Promise<void> {
    const destination = this.resolve(storageKey);
    await mkdir(dirname(destination), { recursive: true });
    try {
      await access(destination);
      await this.removeQuarantine(quarantinePath);
    } catch {
      await rename(quarantinePath, destination);
    }
  }

  resolve(storageKey: string): string {
    if (!/^(audio|image)\/[a-f0-9]{2}\/[a-f0-9]{64}\.[a-z0-9]{1,12}$/.test(storageKey)) {
      throw new Error("Invalid object storage key");
    }
    const path = resolve(this.objectRoot, storageKey);
    if (relative(this.objectRoot, path).startsWith("..")) throw new Error("Object path escapes storage root");
    return path;
  }

  async removeQuarantine(path: string): Promise<void> {
    const resolved = resolve(path);
    if (relative(this.quarantineRoot, resolved).startsWith("..")) throw new Error("Quarantine path escapes storage root");
    await unlink(resolved).catch((error: NodeJS.ErrnoException) => {
      if (error.code !== "ENOENT") throw error;
    });
  }

  async cleanupQuarantine(olderThan: Date): Promise<number> {
    let removed = 0;
    for (const entry of await readdir(this.quarantineRoot, { withFileTypes: true })) {
      if (!entry.isFile()) continue;
      const path = join(this.quarantineRoot, entry.name);
      if ((await stat(path)).mtime < olderThan) {
        await this.removeQuarantine(path);
        removed += 1;
      }
    }
    return removed;
  }

  async cleanupOrphans(referencedKeys: Set<string>, olderThan: Date): Promise<number> {
    let removed = 0;
    for (const kind of ["audio", "image"]) {
      const kindRoot = join(this.objectRoot, kind);
      let prefixes;
      try { prefixes = await readdir(kindRoot, { withFileTypes: true }); } catch { continue; }
      for (const prefix of prefixes) {
        if (!prefix.isDirectory()) continue;
        const prefixRoot = join(kindRoot, prefix.name);
        for (const entry of await readdir(prefixRoot, { withFileTypes: true })) {
          if (!entry.isFile()) continue;
          const key = `${kind}/${prefix.name}/${entry.name}`; const path = join(prefixRoot, entry.name);
          if (!referencedKeys.has(key) && (await stat(path)).mtime < olderThan) {
            await unlink(path); removed += 1;
          }
        }
      }
    }
    return removed;
  }
}
