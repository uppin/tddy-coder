/**
 * Unit tests for the input-delivery sequence: answers to terminal sends are reported in send order,
 * so a late answer to an earlier send cannot overwrite a later one's.
 */

import { describe, expect, it } from "bun:test";
import { inputDeliverySequence } from "./inputDelivery";

function aSequence() {
  const reported: Array<string | null> = [];
  return { reported, next: inputDeliverySequence((failure) => reported.push(failure)) };
}

describe("inputDeliverySequence", () => {
  it("reports sends answered in order, each in turn", () => {
    // Given
    const { reported, next } = aSequence();
    const first = next();
    const second = next();

    // When
    first("refused");
    second(null);

    // Then
    expect(reported).toEqual(["refused", null]);
  });

  it("drops an earlier send's answer that lands after a later send's", () => {
    // Given
    const { reported, next } = aSequence();
    const first = next();
    const second = next();

    // When the later send is answered first
    second(null);
    first("refused");

    // Then
    expect(reported).toEqual([null]);
  });

  it("still reports a later send's answer after an earlier one's", () => {
    // Given
    const { reported, next } = aSequence();
    const first = next();
    const second = next();
    const third = next();

    // When
    second(null);
    first("refused");
    third("refused again");

    // Then
    expect(reported).toEqual([null, "refused again"]);
  });
});
