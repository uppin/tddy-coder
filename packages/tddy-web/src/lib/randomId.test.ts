import { describe, it, expect, afterEach } from "bun:test";
import { randomUuid } from "./randomId";

const realCrypto = globalThis.crypto;

/** Swaps the `crypto` global so the insecure-origin fallbacks are exercised. */
function setCrypto(value: unknown) {
  Object.defineProperty(globalThis, "crypto", { value, configurable: true, writable: true });
}

const UUID_SHAPE = /^[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/;

describe("randomUuid", () => {
  afterEach(() => {
    setCrypto(realCrypto);
  });

  it("returns a v4-shaped id when crypto.randomUUID is available", () => {
    // Given / When
    const id = randomUuid();

    // Then
    expect(id).toMatch(UUID_SHAPE);
  });

  it("returns distinct ids across calls", () => {
    // Given / When
    const ids = new Set(Array.from({ length: 50 }, () => randomUuid()));

    // Then
    expect(ids.size).toBe(50);
  });

  it("still returns a v4-shaped id on an insecure origin, where randomUUID is missing", () => {
    // Given — `crypto.randomUUID` is secure-context only (https / localhost); plain http exposes
    // `getRandomValues` but not `randomUUID`.
    setCrypto({ getRandomValues: realCrypto.getRandomValues.bind(realCrypto) });

    // When
    const id = randomUuid();

    // Then
    expect(id).toMatch(UUID_SHAPE);
  });

  it("returns distinct ids on an insecure origin", () => {
    // Given
    setCrypto({ getRandomValues: realCrypto.getRandomValues.bind(realCrypto) });

    // When
    const ids = new Set(Array.from({ length: 50 }, () => randomUuid()));

    // Then
    expect(ids.size).toBe(50);
  });

  it("returns a v4-shaped id when the whole crypto global is missing", () => {
    // Given
    setCrypto(undefined);

    // When
    const id = randomUuid();

    // Then
    expect(id).toMatch(UUID_SHAPE);
  });

  it("returns an id safe to use as a single path segment", () => {
    // Given — the daemon rejects an upload_id that is not a basename.
    setCrypto({ getRandomValues: realCrypto.getRandomValues.bind(realCrypto) });

    // When
    const id = randomUuid();

    // Then
    expect(id).not.toContain("/");
    expect(id).not.toContain("\\");
    expect(id).not.toContain(".");
  });
});
