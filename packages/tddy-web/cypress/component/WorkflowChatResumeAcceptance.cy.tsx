/**
 * Acceptance: after a browser reload, the tddy-coder workflow chat (`WorkflowChatScreen`, ACP mirror)
 * must **resume** the existing session and repaint its prior conversation — not silently start a fresh,
 * empty session.
 *
 * The reloaded view already knows which session it is (`SessionEntry.sessionId`). ACP's resume path is
 * `session/load` (`LoadSessionRequest`): the client asks the agent to load that session id, and the
 * agent replays the recorded turns as `session/update` notifications (`user_message_chunk` +
 * `agent_message_chunk`) before continuing. Today the chat instead always sends `new_session`, so the
 * agent never replays anything, and even a replayed `user_message_chunk` is dropped on the client —
 * hence the empty chat after reload.
 *
 * PRD: docs/ft/web/session-drawer.md § Per-Workflow Session Views; docs/ft/coder/acp-protobuf-rpc.md.
 */

import React from "react";
import { create } from "@bufbuild/protobuf";
import { anInMemoryRpcBackend } from "tddy-connectrpc-testkit";
import { SessionEntrySchema, type SessionEntry } from "../../src/gen/connection_pb";
import {
  AcpService,
  AcpAgentMessageSchema,
  type AcpClientMessage,
} from "../../src/gen/tddy/acp/v1/acp_pb";
import { WorkflowChatScreen } from "../../src/components/sessions/WorkflowChatScreen";
import { mountWithRpc } from "../support/rpc/inMemory";
import { agentChatPage } from "../support/pages/agentChatPage";

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

/** The session the operator was already chatting in before the reload. */
const RELOADED_SESSION: SessionEntry = create(SessionEntrySchema, {
  sessionId: "resume-workflow-chat-aaaa-0000-0000-000000000042",
  createdAt: "2026-07-01T10:00:00Z",
  status: "active",
  repoPath: "/home/dev/resume-workflow-project",
  isActive: true,
  projectId: "proj-resume-workflow-chat",
  recipe: "tdd",
  sessionType: "tool",
});

const PRIOR_USER_TURN = "Add a rate limiter to the login endpoint.";
const PRIOR_AGENT_TURN = "Sure — I'll add a token-bucket rate limiter and cover it with tests.";

// ---------------------------------------------------------------------------
// ACP frame builders (server → client, `session/update` notifications)
// ---------------------------------------------------------------------------

/** One `session/update` carrying a replayed **agent** turn. */
function agentTurn(text: string) {
  return create(AcpAgentMessageSchema, {
    id: 0n,
    msg: {
      case: "sessionUpdate",
      value: {
        sessionId: { value: RELOADED_SESSION.sessionId },
        update: {
          update: {
            case: "agentMessageChunk",
            value: { content: { block: { case: "text", value: { text } } } },
          },
        },
      },
    },
  });
}

/** One `session/update` carrying a replayed **user** turn (the operator's earlier prompt). */
function userTurn(text: string) {
  return create(AcpAgentMessageSchema, {
    id: 0n,
    msg: {
      case: "sessionUpdate",
      value: {
        sessionId: { value: RELOADED_SESSION.sessionId },
        update: {
          update: {
            case: "userMessageChunk",
            value: { content: { block: { case: "text", value: { text } } } },
          },
        },
      },
    },
  });
}

beforeEach(() => {
  cy.viewport(1280, 800);
});

it("resumes the reloaded session by asking the agent to load its id, not by starting a new session", () => {
  // Given — an ACP agent that only replays a turn once it is asked to LOAD the reloaded session id.
  // A `new_session` request (the current behaviour) never triggers this, so the bubble stays absent.
  async function* session(requests: AsyncIterable<AcpClientMessage>) {
    for await (const msg of requests) {
      if (msg.msg.case === "loadSession") {
        const loadedId = msg.msg.value.sessionId?.value ?? "";
        yield agentTurn(`Resumed ${loadedId}`);
        return;
      }
    }
  }
  const backend = anInMemoryRpcBackend().implement(AcpService, { session });

  // When — the reloaded tddy-coder workflow chat mounts for that session
  mountWithRpc(<WorkflowChatScreen session={RELOADED_SESSION} />, backend);

  // Then — the client asked to load THIS session's id, proven by the agent's id-bearing confirmation
  agentChatPage.chatMessage(0).should("have.text", `Resumed ${RELOADED_SESSION.sessionId}`);
});

it("repaints the prior conversation (user and agent turns) the agent replays on resume", () => {
  // Given — an ACP agent that, on resume, replays the earlier user prompt then the earlier agent reply
  async function* session() {
    yield userTurn(PRIOR_USER_TURN);
    yield agentTurn(PRIOR_AGENT_TURN);
  }
  const backend = anInMemoryRpcBackend().implement(AcpService, { session });

  // When — the reloaded tddy-coder workflow chat mounts for that session
  mountWithRpc(<WorkflowChatScreen session={RELOADED_SESSION} />, backend);

  // Then — both prior turns reappear, in order, as their own bubbles (the user turn is not dropped)
  agentChatPage.chatMessage(0).should("have.text", PRIOR_USER_TURN);
  agentChatPage.chatMessageKind(0).should("equal", "user");
  agentChatPage.chatMessage(1).should("have.text", PRIOR_AGENT_TURN);
  agentChatPage.chatMessageKind(1).should("equal", "agent");
});
