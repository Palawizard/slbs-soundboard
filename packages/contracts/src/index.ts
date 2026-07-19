import { z } from "zod";

export const healthResponseSchema = z.object({
  status: z.literal("ok"),
  service: z.literal("community-api"),
});

export type HealthResponse = z.infer<typeof healthResponseSchema>;

