import { describe, expect, test } from "bun:test";
import { presenceIdentityForUser } from "../lib/presenceIdentity";

describe("presence identity must be unique per tab to avoid DUPLICATE_IDENTITY disconnect", () => {
  test("two concurrent connections for the same user should produce different identities", () => {
    const tab1Identity = presenceIdentityForUser("testuser");
    const tab2Identity = presenceIdentityForUser("testuser");
    expect(tab1Identity).not.toBe(tab2Identity);
  });

  test("presence identity should contain a tab-unique component", () => {
    const identity = presenceIdentityForUser("myuser");
    expect(identity).toMatch(/\d{10,}/);
  });
});
