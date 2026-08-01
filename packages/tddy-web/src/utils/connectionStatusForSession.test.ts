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

  it("returns 'disconnected' for a dead session still carrying a pending elicitation", () => {
    // Given — `pendingElicitation` is persisted and is not cleared when the agent dies, so a dead
    // session can still carry it. Nothing is waiting on the operator, so it reads as disconnected.
    const session = anInactiveSession({ isActive: false, pendingElicitation: true });

    // When
    const status = connectionStatusForSession(session);

    // Then
    expect(status).toBe("disconnected");
  });

  it("liveness decides before the elicitation flag refines it", () => {
    // Given — the same pendingElicitation flag on a live and a dead session
    const activeEliciting = anActiveSession({ isActive: true, pendingElicitation: true });
    const inactiveEliciting = anInactiveSession({ isActive: false, pendingElicitation: true });

    // When
    const statusFromActive = connectionStatusForSession(activeEliciting);
    const statusFromInactive = connectionStatusForSession(inactiveEliciting);

    // Then — only the live one is actually waiting on an answer
    expect(statusFromActive).toBe("needs-input");
    expect(statusFromInactive).toBe("disconnected");
  });
});
