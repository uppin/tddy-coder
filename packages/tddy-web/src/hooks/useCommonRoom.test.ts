import { describe, expect, it } from "bun:test";
import { presenceIdentityForUser } from "../lib/presenceIdentity";
import { withMockedSessionStorage } from "../test-utils/sessionStorage";

describe("presence identity for LiveKit common room", () => {
  it("repeated calls in the same tab reuse the same identity via sessionStorage", () => {
    withMockedSessionStorage(() => {
      // Given — fresh tab (empty sessionStorage)

      // When
      const a = presenceIdentityForUser("testuser");
      const b = presenceIdentityForUser("testuser");

      // Then — both calls return the same stable identity for this tab
      expect(a).toBe(b);
      expect(a.startsWith("web-testuser-")).toBe(true);
    });
  });

  it("different GitHub logins get different identities in the same tab", () => {
    withMockedSessionStorage(() => {
      // Given — fresh tab

      // When
      const alice = presenceIdentityForUser("alice");
      const bob = presenceIdentityForUser("bob");

      // Then
      expect(alice).not.toBe(bob);
    });
  });

  it("identity includes a time-derived segment for debugging", () => {
    withMockedSessionStorage(() => {
      // Given — fresh tab

      // When
      const identity = presenceIdentityForUser("myuser");

      // Then — time-derived segment is generated at runtime; can only validate it contains a number sequence
      expect(identity).toMatch(/\d{10,}/);
    });
  });
});
