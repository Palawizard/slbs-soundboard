import { resolve } from "node:path";
import { z } from "zod";

const environmentSchema = z.object({
  NODE_ENV: z.enum(["development", "test", "production"]).default("development"),
  HOST: z.string().default("127.0.0.1"),
  PORT: z.coerce.number().int().min(1).max(65_535).default(3000),
  DATABASE_URL: z.string().min(1),
  MEDIA_ROOT: z.string().min(1).default("./data/media"),
  PUBLIC_BASE_URL: z.string().url().default("http://127.0.0.1:3000"),
  GOOGLE_CLIENT_ID: z.string().min(1),
  GOOGLE_CLIENT_SECRET: z.string().min(1).optional(),
  SESSION_SECRET: z.string().min(32),
  FFMPEG_PROBE_PATH: z.string().min(1).default("ffprobe"),
  FFMPEG_PATH: z.string().min(1).default("ffmpeg"),
  TRUST_PROXY: z.enum(["true", "false"]).default("false"),
  ADMIN_TOKEN: z.string().min(32).optional(),
});

export type ApiConfig = {
  environment: "development" | "test" | "production";
  host: string;
  port: number;
  databaseUrl: string;
  mediaRoot: string;
  publicBaseUrl: string;
  googleClientId: string;
  googleClientSecret?: string;
  sessionSecret: string;
  ffprobePath: string;
  ffmpegPath: string;
  trustProxy: boolean;
  adminToken?: string;
};

export function loadConfig(environment: NodeJS.ProcessEnv = process.env): ApiConfig {
  const parsed = environmentSchema.parse(environment);
  return {
    environment: parsed.NODE_ENV,
    host: parsed.HOST,
    port: parsed.PORT,
    databaseUrl: parsed.DATABASE_URL,
    mediaRoot: resolve(parsed.MEDIA_ROOT),
    publicBaseUrl: parsed.PUBLIC_BASE_URL.replace(/\/$/, ""),
    googleClientId: parsed.GOOGLE_CLIENT_ID,
    ...(parsed.GOOGLE_CLIENT_SECRET ? { googleClientSecret: parsed.GOOGLE_CLIENT_SECRET } : {}),
    sessionSecret: parsed.SESSION_SECRET,
    ffprobePath: parsed.FFMPEG_PROBE_PATH,
    ffmpegPath: parsed.FFMPEG_PATH,
    trustProxy: parsed.TRUST_PROXY === "true",
    ...(parsed.ADMIN_TOKEN ? { adminToken: parsed.ADMIN_TOKEN } : {}),
  };
}
