import { createHash } from "node:crypto";
import { describe, expect, it, vi } from "vitest";
import { AuthService, validateLoopbackRedirect } from "../src/auth/service.js";
import type { CommunityRepository, UserRecord } from "../src/domain.js";
import type { GoogleIdentityProvider } from "../src/auth/google.js";

const now = new Date("2026-07-19T12:00:00.000Z");
const user: UserRecord = { id: "123e4567-e89b-12d3-a456-426614174000", googleSubject: "google-1", email: "user@example.com", username: "Utilisateur", avatarUrl: null, disabledAt: null, createdAt: now };
const verifier = "v".repeat(43);
const challenge = createHash("sha256").update(verifier).digest("base64url");

function setup() {
  let transaction: { redirectUri: string; codeChallenge: string; nonce: string; expiresAt: Date } | null = null;
  const repository = {
    createAuthTransaction: vi.fn(async (_hash, value) => { transaction = value; }),
    consumeAuthTransaction: vi.fn(async () => { const value = transaction; transaction = null; return value; }),
    upsertGoogleUser: vi.fn(async () => user),
    createSession: vi.fn(async () => undefined),
    authenticateSession: vi.fn(async () => user),
    revokeSession: vi.fn(async () => true),
  } as unknown as CommunityRepository;
  const google: GoogleIdentityProvider = {
    authorizationUrl: vi.fn(() => "https://accounts.google.com/o/oauth2/v2/auth?state=test"),
    exchangeCode: vi.fn(async () => ({ subject: "google-1", email: "user@example.com", emailVerified: true, name: "Utilisateur", avatarUrl: null })),
  };
  return { service: new AuthService(repository, google, "s".repeat(32), () => now), repository, google };
}

describe("desktop Google authentication", () => {
  it("accepts only explicit loopback callback addresses", () => {
    expect(() => validateLoopbackRedirect("http://127.0.0.1:49152/callback")).not.toThrow();
    expect(() => validateLoopbackRedirect("https://example.com/callback")).toThrow();
    expect(() => validateLoopbackRedirect("http://localhost:49152/callback")).toThrow();
  });

  it("uses PKCE and consumes the transaction exactly once", async () => {
    const { service, repository, google } = setup();
    const started = await service.start({ redirectUri: "http://127.0.0.1:49152/callback", codeChallenge: challenge });
    const session = await service.exchange({ state: started.state, code: "google-code", codeVerifier: verifier, redirectUri: "http://127.0.0.1:49152/callback" });
    expect(session.user.username).toBe("Utilisateur");
    expect(repository.createSession).toHaveBeenCalledOnce();
    expect(google.exchangeCode).toHaveBeenCalledOnce();
    await expect(service.exchange({ state: started.state, code: "google-code", codeVerifier: verifier, redirectUri: "http://127.0.0.1:49152/callback" })).rejects.toThrow("expiré");
  });

  it("rejects an invalid proof before contacting Google", async () => {
    const { service, google } = setup();
    const started = await service.start({ redirectUri: "http://127.0.0.1:49152/callback", codeChallenge: challenge });
    await expect(service.exchange({ state: started.state, code: "google-code", codeVerifier: "x".repeat(43), redirectUri: "http://127.0.0.1:49152/callback" })).rejects.toThrow("preuve");
    expect(google.exchangeCode).not.toHaveBeenCalled();
  });
});
