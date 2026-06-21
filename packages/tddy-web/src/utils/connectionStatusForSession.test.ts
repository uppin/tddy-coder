import { describe, expect, it } from "bun:test";
import { anActiveSession, anInactiveSession } from "../test-utils";
import { connectionStatusForSession } from "./connectionStatusForSession";

describe("connectionStatusForSession — maps session proto fields to a display status token", () => {
  it("returns 'connected' for an active session with no pending elicitation", () => {
    // Given
    const session = anActiveSession({ isActive: true, pendingElicitation: false });

    // When
    const status = connectionStatusForSession(session);

    // Then
    expect(status).toBe("connected");
  });

  it("returns 'disconnected' for an inactive session with no pending elicitation", () => {
    // Given
    const session = anInactiveSession({ isActive: false, pendingElicitation: false });

    // When
    const status = connectionStatusForSession(session);

    // Then
    expect(status).toBe("disconnected");
  });

  it("returns 'needs-input' for an active session that has pending elicitation", () => {
    // Given
    const session = anActiveSession({ isActive: true, pendingElicitation: true });

    // When
    const status = connectionStatusForSession(session);

    // Then
    expect(status).toBe("needs-input");
  });

  it("returns 'needs-input' for an inactive session that has pending elicitation", () => {
    // Given — elicitation can be pending on an already-dead session (state not cleared)
    const session = anInactiveSession({ isActive: false, pendingElicitation: true });

    // When
    const status = connectionStatusForSession(session);

    // Then
    expect(status).toBe("needs-input");
  });

  it("pendingElicitation takes precedence over isActive for the needs-input token", () => {
    // Given — both active variants with pendingElicitation true must yield needs-input
    const activeEliciting = anActiveSession({ isActive: true, pendingElicitation: true });
    const inactiveEliciting = anInactiveSession({ isActive: false, pendingElicitation: true });

    // When
    const statusFromActive = connectionStatusForSession(activeEliciting);
    const statusFromInactive = connectionStatusForSession(inactiveEliciting);

    // Then
    expect(statusFromActive).toBe("needs-input");
    expect(statusFromInactive).toBe("needs-input");
  });
});
