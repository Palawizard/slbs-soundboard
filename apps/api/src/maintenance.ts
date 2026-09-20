import type { CommunityRepository } from "./domain.js";
import type { ObjectStorage } from "./media/storage.js";

const HOUR_MS = 60 * 60 * 1000;

export class MaintenanceService {
  constructor(
    private readonly repository: CommunityRepository,
    private readonly storage: ObjectStorage,
    private readonly now: () => Date = () => new Date(),
  ) {}

  async run(): Promise<{ authTransactions: number; sessions: number; quarantine: number; orphanObjects: number }> {
    const now = this.now();
    const expired = await this.repository.cleanupExpired(now);
    const quarantine = await this.storage.cleanupQuarantine(new Date(now.getTime() - HOUR_MS));
    const keys = await this.repository.referencedStorageKeys();
    const orphanObjects = await this.storage.cleanupOrphans(keys, new Date(now.getTime() - 24 * HOUR_MS));
    return { ...expired, quarantine, orphanObjects };
  }

  start(intervalMs = 15 * 60 * 1000): () => void {
    const timer = setInterval(() => { void this.run().catch(() => undefined); }, intervalMs);
    timer.unref();
    return () => clearInterval(timer);
  }
}
