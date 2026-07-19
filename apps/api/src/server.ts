import Fastify, { type FastifyInstance } from "fastify";
import { healthResponseSchema } from "@slb/contracts";

export function buildServer(): FastifyInstance {
  const server = Fastify({ logger: true });

  server.get("/health", async () =>
    healthResponseSchema.parse({
      status: "ok",
      service: "community-api",
    }),
  );

  return server;
}

