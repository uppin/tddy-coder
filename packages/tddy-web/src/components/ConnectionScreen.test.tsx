import { describe, expect, it } from "bun:test";
import {
  attachKindForSessionControl,
  nextPresentationFromAttach,
} from "./connection/terminalPresentation";

/**
 * Smoke: ConnectionScreen production module must keep importing the presentation helpers used
 * for start/connect vs resume branching. Detailed behaviour is covered in
 * `terminalPresentation.test.ts`.
 */
describe("ConnectionScreen — terminal presentation import contract", () => {
  it("resumeSession maps to the reconnect attach kind", () => {
    // Then
    expect(attachKindForSessionControl("resumeSession")).toBe("reconnect");
  });

  it("reconnecting from hidden state opens the overlay presentation", () => {
    // When
    const result = nextPresentationFromAttach("hidden", "reconnect");

    // Then
    expect(result.presentation).toBe("overlay");
  });

  it("opening a new session from hidden state does not push a terminal route", () => {
    // When
    const result = nextPresentationFromAttach("hidden", "new");

    // Then
    expect(result.shouldPushTerminalRoute).toBe(false);
  });
});
