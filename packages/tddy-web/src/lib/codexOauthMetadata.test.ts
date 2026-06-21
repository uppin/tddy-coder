import { describe, expect, it } from "bun:test";
import { aCodexOAuthMeta } from "../test-utils";
import { parseCodexOAuthMetadata } from "./codexOauthMetadata";

describe("parseCodexOAuthMetadata", () => {
  it("returns null for an empty string", () => {
    // When / Then
    expect(parseCodexOAuthMetadata("")).toBeNull();
  });

  it("returns null for a whitespace-only string", () => {
    // When / Then
    expect(parseCodexOAuthMetadata("   ")).toBeNull();
  });

  it("returns null for invalid JSON", () => {
    // When / Then
    expect(parseCodexOAuthMetadata("{")).toBeNull();
  });

  it("returns a non-null object for a well-formed pending oauth metadata", () => {
    // Given
    const meta = aCodexOAuthMeta({ pending: true, authorizeUrl: "https://auth.openai.com/x", callbackPort: 8765, state: "s1" });

    // When
    const result = parseCodexOAuthMetadata(meta);

    // Then
    expect(result).not.toBeNull();
  });

  it("parses the pending flag from the metadata", () => {
    // Given
    const meta = aCodexOAuthMeta({ pending: true, authorizeUrl: "https://auth.openai.com/x", callbackPort: 8765, state: "s1" });

    // When
    const result = parseCodexOAuthMetadata(meta);

    // Then
    expect(result!.pending).toBe(true);
  });

  it("parses the authorize URL from the metadata", () => {
    // Given
    const meta = aCodexOAuthMeta({ pending: true, authorizeUrl: "https://auth.openai.com/x", callbackPort: 8765, state: "s1" });

    // When
    const result = parseCodexOAuthMetadata(meta);

    // Then
    expect(result!.authorizeUrl).toBe("https://auth.openai.com/x");
  });

  it("parses the callback port as a number from the metadata", () => {
    // Given
    const meta = aCodexOAuthMeta({ pending: true, authorizeUrl: "https://auth.openai.com/x", callbackPort: 8765, state: "s1" });

    // When
    const result = parseCodexOAuthMetadata(meta);

    // Then
    expect(result!.callbackPort).toBe(8765);
  });

  it("parses the state token from the metadata", () => {
    // Given
    const meta = aCodexOAuthMeta({ pending: true, authorizeUrl: "https://auth.openai.com/x", callbackPort: 8765, state: "s1" });

    // When
    const result = parseCodexOAuthMetadata(meta);

    // Then
    expect(result!.state).toBe("s1");
  });
});
