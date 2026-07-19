import { describe, expect, it } from "vitest";
import {
  authStartRequestSchema,
  createPublicationRequestSchema,
  publicationListQuerySchema,
  usernameSchema,
} from "./index.js";

describe("community API contracts", () => {
  it("accepts only loopback-shaped auth payload primitives", () => {
    const payload = authStartRequestSchema.parse({
      redirectUri: "http://127.0.0.1:49152/callback",
      codeChallenge: "a".repeat(43),
    });
    expect(payload.codeChallenge).toHaveLength(43);
  });

  it("normalizes pagination limits and rejects unsafe usernames", () => {
    expect(publicationListQuerySchema.parse({ limit: "50" }).limit).toBe(50);
    expect(usernameSchema.safeParse("nom valide").success).toBe(false);
    expect(usernameSchema.parse("SLB_42")).toBe("SLB_42");
  });

  it("applies publication defaults", () => {
    const result = createPublicationRequestSchema.parse({
      title: "Cloche",
      audioMediaId: "123e4567-e89b-12d3-a456-426614174000",
    });
    expect(result).toMatchObject({ description: "", imageMediaId: null });
  });
});
