import { describe, it, expect } from "bun:test";
import { safeTestIdPart } from "./testId";

/**
 * Components and `cypress/support/testIds.ts` both build dynamic test ids through this function, so
 * its exact output is a contract between them rather than an implementation detail.
 */
describe("safeTestIdPart", () => {
  it("collapses the dot in a LiveKit room name", () => {
    // Given / When
    const part = safeTestIdPart("livekit.common_room");

    // Then
    expect(part).toEqual("livekit_common_room");
  });

  it("keeps letters, digits, underscores and hyphens as they are", () => {
    // Given / When
    const part = safeTestIdPart("daemon-pr-stack-presenter-room-0001_v2");

    // Then
    expect(part).toEqual("daemon-pr-stack-presenter-room-0001_v2");
  });

  it("collapses each unsafe character to its own underscore", () => {
    // Given / When
    const part = safeTestIdPart("daemon-x/y z@1");

    // Then
    expect(part).toEqual("daemon-x_y_z_1");
  });

  it("returns an empty string for an empty part", () => {
    // Given / When
    const part = safeTestIdPart("");

    // Then
    expect(part).toEqual("");
  });
});
