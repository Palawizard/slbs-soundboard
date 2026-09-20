import { createRemoteJWKSet, jwtVerify } from "jose";
import type { GoogleIdentity } from "../domain.js";

const authorizationEndpoint = "https://accounts.google.com/o/oauth2/v2/auth";
const tokenEndpoint = "https://oauth2.googleapis.com/token";
const googleKeys = createRemoteJWKSet(new URL("https://www.googleapis.com/oauth2/v3/certs"));

export type GoogleAuthorizationInput = {
  state: string;
  redirectUri: string;
  codeChallenge: string;
  nonce: string;
};

export interface GoogleIdentityProvider {
  authorizationUrl(input: GoogleAuthorizationInput): string;
  exchangeCode(input: { code: string; codeVerifier: string; redirectUri: string; nonce: string }): Promise<GoogleIdentity>;
}

export class ProductionGoogleIdentityProvider implements GoogleIdentityProvider {
  constructor(
    private readonly clientId: string,
    private readonly clientSecret?: string,
  ) {}

  authorizationUrl(input: GoogleAuthorizationInput): string {
    const url = new URL(authorizationEndpoint);
    url.search = new URLSearchParams({
      client_id: this.clientId,
      redirect_uri: input.redirectUri,
      response_type: "code",
      scope: "openid email profile",
      state: input.state,
      nonce: input.nonce,
      code_challenge: input.codeChallenge,
      code_challenge_method: "S256",
      prompt: "select_account",
    }).toString();
    return url.toString();
  }

  async exchangeCode(input: { code: string; codeVerifier: string; redirectUri: string; nonce: string }): Promise<GoogleIdentity> {
    const body = new URLSearchParams({
      client_id: this.clientId,
      code: input.code,
      code_verifier: input.codeVerifier,
      grant_type: "authorization_code",
      redirect_uri: input.redirectUri,
    });
    if (this.clientSecret) body.set("client_secret", this.clientSecret);
    const response = await fetch(tokenEndpoint, {
      method: "POST",
      headers: { "content-type": "application/x-www-form-urlencoded", accept: "application/json" },
      body,
      signal: AbortSignal.timeout(10_000),
    });
    if (!response.ok) throw new GoogleAuthenticationError("Google a refusé le code d’autorisation.");
    const tokens = await response.json() as { id_token?: unknown };
    if (typeof tokens.id_token !== "string") throw new GoogleAuthenticationError("Google n’a pas fourni d’identité vérifiable.");
    const { payload } = await jwtVerify(tokens.id_token, googleKeys, {
      audience: this.clientId,
      issuer: ["https://accounts.google.com", "accounts.google.com"],
      algorithms: ["RS256"],
    });
    if (payload.nonce !== input.nonce || typeof payload.sub !== "string" || typeof payload.email !== "string" || payload.email_verified !== true) {
      throw new GoogleAuthenticationError("L’identité Google reçue n’est pas valide.");
    }
    return {
      subject: payload.sub,
      email: payload.email,
      emailVerified: true,
      name: typeof payload.name === "string" ? payload.name : null,
      avatarUrl: typeof payload.picture === "string" ? payload.picture : null,
    };
  }
}

export class GoogleAuthenticationError extends Error {}
