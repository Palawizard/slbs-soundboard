import { afterEach, describe, expect, it, vi } from "vitest";
import { buildServer, type ServerDependencies } from "../src/server.js";
import type { ApiConfig } from "../src/config.js";

const servers = [] as ReturnType<typeof buildServer>[];
afterEach(async () => { await Promise.all(servers.splice(0).map((server) => server.close())); });

function dependencies(databaseReady: boolean): ServerDependencies {
  const config: ApiConfig = {
    environment: "test", host: "127.0.0.1", port: 3000, databaseUrl: "postgres://test",
    mediaRoot: "data", publicBaseUrl: "https://community.example.test", googleClientId: "google",
    sessionSecret: "s".repeat(32), ffprobePath: "ffprobe", trustProxy: false,
  };
  return {
    config,
    repository: { health: vi.fn(async () => databaseReady) } as unknown as ServerDependencies["repository"],
    auth: {} as ServerDependencies["auth"], media: {} as ServerDependencies["media"],
    publications: {} as ServerDependencies["publications"], storage: {} as ServerDependencies["storage"],
  };
}

describe("health endpoint", () => {
  it("reports database readiness", async () => {
    const server = buildServer(dependencies(true)); servers.push(server);
    const response = await server.inject({ method: "GET", url: "/health" });
    expect(response.statusCode).toBe(200);
    expect(response.json()).toEqual({ status: "ok", service: "community-api", database: "ready" });
  });

  it("fails readiness while PostgreSQL is unavailable", async () => {
    const server = buildServer(dependencies(false)); servers.push(server);
    expect((await server.inject({ method: "GET", url: "/health" })).statusCode).toBe(503);
  });
});
