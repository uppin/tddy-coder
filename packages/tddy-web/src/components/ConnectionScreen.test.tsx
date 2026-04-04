import { describe, expect, it } from "bun:test";
import {
  attachKindForSessionControl,
  nextPresentationFromAttach,
} from "./connection/terminalPresentation";

/**
 * Smoke: ConnectionScreen production module must keep importing presentation helpers used for
 * start/connect vs resume branching. Detailed behavior is covered in `terminalPresentation.test.ts`.
 */
describe("ConnectionScreen — terminal presentation import contract", () => {
  it("exposes expected attach → presentation mapping used by ConnectionScreen handlers", () => {
    expect(attachKindForSessionControl("resumeSession")).toBe("reconnect");
    expect(nextPresentationFromAttach("hidden", "reconnect").presentation).toBe("overlay");
    expect(nextPresentationFromAttach("hidden", "new").shouldPushTerminalRoute).toBe(true);
  });
});
