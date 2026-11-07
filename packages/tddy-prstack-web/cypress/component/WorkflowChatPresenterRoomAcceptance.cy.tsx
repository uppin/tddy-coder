/**
 * Acceptance test: the full-screen Workflow Chat Screen must open its own dedicated presenter room
 * for the session's remote Presenter, derived from the session's own attachment hint — the same
 * room the terminal independently connects to. If it sat on a room handed down from above (the
 * always-null VNC-purpose `room` of the main pane), the chat could never send or receive anything
 * (the exact bug that was fixed for the pr-stack chat).
 *
 * The room itself is the media port's: the screen asks it for the room the hint names.
 *
 * PRD: docs/ft/web/session-drawer.md § Per-Workflow Session Views.
 */

import React from "react";
import { create } from "@bufbuild/protobuf";
import { anInMemoryRpcBackend } from "tddy-connectrpc-testkit";
import type { SessionAttachmentHint } from "tddy-client-web";
import { SessionEntrySchema } from "tddy-client-web/gen/session_pb";
import { WorkflowChatScreen } from "../../src/WorkflowChatScreen";
import { anAttachmentHint, mountWithPresenterRoom } from "../support/mountWorkflowChat";

const TDD_SESSION = create(SessionEntrySchema, {
  sessionId: "tdd-presenter-room-aaaa-0000-0000-000000000001",
  createdAt: "2026-07-01T10:00:00Z",
  status: "active",
  repoPath: "/home/dev/workflow-chat-project",
  isActive: true,
  projectId: "proj-workflow-chat-presenter-room",
  recipe: "tdd",
  sessionType: "tool",
});

/** The hints the screen asked the media port to open a presenter room for. */
function aPresenterRoomRequestLog() {
  const hints: Array<SessionAttachmentHint | null> = [];
  return {
    hints,
    asksFor: (hint: SessionAttachmentHint | null) => {
      hints.push(hint);
      return { room: null, status: "connecting" as const, error: null };
    },
  };
}

describe("Workflow Chat Screen — opens its own presenter room from the session's attachment", () => {
  it("asks the media port for the room the session's attachment names, not a room from above", () => {
    // Given — a tool workflow session attached over a room, and a media port that records each request
    const hint = anAttachmentHint(TDD_SESSION.sessionId);
    const requests = aPresenterRoomRequestLog();

    // When
    mountWithPresenterRoom(
      <WorkflowChatScreen session={TDD_SESSION} attachmentHint={hint} />,
      anInMemoryRpcBackend(),
      { usePresenterRoom: requests.asksFor },
    );

    // Then — the presenter connection targets the session's own attached room
    cy.wrap(requests.hints).should("deep.include", hint);
  });
});
