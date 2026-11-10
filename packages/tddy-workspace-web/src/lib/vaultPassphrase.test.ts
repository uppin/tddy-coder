/**
 * Unit tests for the credential vault passphrase rule the prompt applies before the daemon does.
 *
 * The daemon counts a passphrase in characters — Unicode code points (`str::chars` in Rust) — so
 * the page must count the same way. `String.length` counts UTF-16 code units, and every character
 * outside the Basic Multilingual Plane is two of them: a passphrase of four emoji is eight units
 * but four characters, and the daemon refuses it.
 */

import { describe, it, expect } from "bun:test";

import { MAX_PASSPHRASE_CHARS, MIN_PASSPHRASE_CHARS, isAcceptableNewPassphrase, passphraseChars } from "./vaultPassphrase";

const FOUR_EMOJI = "\u{1F511}\u{1F512}\u{1F513}\u{1F510}";

describe("a credential vault passphrase", () => {
  it("is counted in characters, not UTF-16 code units", () => {
    // Given / When
    const counted = passphraseChars(FOUR_EMOJI);

    // Then
    expect(counted).toBe(4);
  });

  it("of fewer characters than the minimum is not accepted, however many code units it takes", () => {
    // Given — eight UTF-16 code units, but only four characters
    const tooShort = FOUR_EMOJI;

    // When
    const accepted = isAcceptableNewPassphrase(tooShort);

    // Then
    expect(accepted).toBe(false);
  });

  it("of exactly the minimum number of characters is accepted", () => {
    // Given
    const justLongEnough = "x".repeat(MIN_PASSPHRASE_CHARS);

    // When
    const accepted = isAcceptableNewPassphrase(justLongEnough);

    // Then
    expect(accepted).toBe(true);
  });

  it("longer than the maximum is not accepted", () => {
    // Given
    const tooLong = "x".repeat(MAX_PASSPHRASE_CHARS + 1);

    // When
    const accepted = isAcceptableNewPassphrase(tooLong);

    // Then
    expect(accepted).toBe(false);
  });
});
