import { afterEach, describe, expect, it } from "vitest";
import { buildServer } from "../src/server.js";

const servers = [] as ReturnType<typeof buildServer>[];

afterEach(async () => {
  await Promise.all(servers.splice(0).map((server) => server.close()));
});

describe("health endpoint", () => {
  it("reports that the API is ready", async () => {
    const server = buildServer();
    servers.push(server);

    const response = await server.inject({ method: "GET", url: "/health" });

    expect(response.statusCode).toBe(200);
    expect(response.json()).toEqual({
      status: "ok",
      service: "community-api",
    });
  });
});

