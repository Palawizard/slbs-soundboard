import type { AuthExchangeRequest, AuthStartRequest, AuthStartResponse, PublicUser, SessionResponse } from "@slb/contracts";
import type { CommunityRepository, UserRecord } from "../domain.js";
import { DomainError } from "../domain.js";
import { keyedHash, randomToken, secureEqual, sha256Base64Url } from "../security.js";
import type { GoogleIdentityProvider } from "./google.js";

const AUTH_TRANSACTION_LIFETIME_MS = 10 * 60 * 1000;
const SESSION_LIFETIME_MS = 12 * 60 * 60 * 1000;

export class AuthService {
  constructor(
    private readonly repository: CommunityRepository,
    private readonly google: GoogleIdentityProvider,
    private readonly sessionSecret: string,
    private readonly now: () => Date = () => new Date(),
  ) {}

  async start(input: AuthStartRequest): Promise<AuthStartResponse> {
    validateLoopbackRedirect(input.redirectUri);
    const state = randomToken();
    const nonce = randomToken();
    const expiresAt = new Date(this.now().getTime() + AUTH_TRANSACTION_LIFETIME_MS);
    await this.repository.createAuthTransaction(keyedHash(this.sessionSecret, state), {
      redirectUri: input.redirectUri,
      codeChallenge: input.codeChallenge,
      nonce,
      expiresAt,
    });
    return {
      authorizationUrl: this.google.authorizationUrl({ ...input, state, nonce }),
      state,
      expiresAt: expiresAt.toISOString(),
    };
  }

  async exchange(input: AuthExchangeRequest): Promise<SessionResponse> {
    validateLoopbackRedirect(input.redirectUri);
    const transaction = await this.repository.consumeAuthTransaction(keyedHash(this.sessionSecret, input.state), this.now());
    if (!transaction || transaction.redirectUri !== input.redirectUri) throw new DomainError("invalid", "Cette connexion a expiré ou a déjà été utilisée.");
    const challenge = sha256Base64Url(input.codeVerifier);
    if (!secureEqual(challenge, transaction.codeChallenge)) throw new DomainError("invalid", "La preuve de connexion n’est pas valide.");
    const identity = await this.google.exchangeCode({ code: input.code, codeVerifier: input.codeVerifier, redirectUri: input.redirectUri, nonce: transaction.nonce });
    if (!identity.emailVerified) throw new DomainError("forbidden", "L’adresse Google doit être vérifiée.");
    const user = await this.repository.upsertGoogleUser(identity);
    if (user.disabledAt) throw new DomainError("forbidden", "Ce compte est désactivé.");
    const token = randomToken();
    const expiresAt = new Date(this.now().getTime() + SESSION_LIFETIME_MS);
    await this.repository.createSession(user.id, keyedHash(this.sessionSecret, token), expiresAt);
    return { token, expiresAt: expiresAt.toISOString(), user: publicUser(user) };
  }

  async authenticate(header: string | undefined): Promise<UserRecord> {
    const match = header ? /^Bearer ([A-Za-z0-9_-]{43,128})$/.exec(header) : null;
    if (!match?.[1]) throw new DomainError("forbidden", "Une connexion est requise.");
    const user = await this.repository.authenticateSession(keyedHash(this.sessionSecret, match[1]), this.now());
    if (!user) throw new DomainError("forbidden", "La session est invalide ou expirée.");
    return user;
  }

  async logout(header: string | undefined): Promise<void> {
    const match = header ? /^Bearer ([A-Za-z0-9_-]{43,128})$/.exec(header) : null;
    if (match?.[1]) await this.repository.revokeSession(keyedHash(this.sessionSecret, match[1]));
  }
}

export function publicUser(user: UserRecord): PublicUser {
  return { id: user.id, username: user.username, avatarUrl: user.avatarUrl, createdAt: user.createdAt.toISOString() };
}

export function validateLoopbackRedirect(value: string): void {
  const url = new URL(value);
  const loopback = url.hostname === "127.0.0.1" || url.hostname === "[::1]";
  if (url.protocol !== "http:" || !loopback || !url.port || url.username || url.password || url.hash) {
    throw new DomainError("invalid", "L’adresse de retour de connexion n’est pas autorisée.");
  }
}
