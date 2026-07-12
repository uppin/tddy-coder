/**
 * In-memory backend + fixtures for the Session Inspector "Usage" tab acceptance tests.
 *
 * Token usage rides the existing generic `ServerMessage` oneof on `TddyRemote.Stream` (a new
 * `tokenUsageUpdated` variant carrying the full cumulative `ConversationRecord` snapshot). These
 * helpers drive that stream through `anInMemoryRpcBackend()` + `mountWithRpc` — the
 * fluent-tests-preferred in-memory fake — exactly as `PrStackChatStreamingAcceptance` drives the
 * chat's `agentOutput` events.
 *
 * Each element of `snapshots` is one full cumulative snapshot (a list of conversations); the
 * stream yields one `ServerMessage` per snapshot, in order, then closes — so the view renders the
 * latest snapshot.
 */

import { create } from "@bufbuild/protobuf";
import type { InMemoryRpcBackend } from "tddy-connectrpc-testkit";
import type { SessionEntry } from "../../../src/gen/connection_pb";
import { TddyRemote, ServerMessageSchema } from "../../../src/gen/tddy/v1/remote_pb";
import { aSessionsDrawerBackend } from "./vncBackend";

// ---------------------------------------------------------------------------
// ConversationRecord fixture
// ---------------------------------------------------------------------------

/**
 * Wire shape of one conversation's accounting (matches proto `ConversationRecord`; 64-bit token
 * counts are `bigint`, `turns` is a 32-bit `number`).
 */
export interface ConversationRecordInit {
  agent: string;
  id: string;
  model: string;
  inputTokens: bigint;
  outputTokens: bigint;
  totalTokens: bigint;
  turns: number;
}

/** A conversation record; override only the fields that matter for the scenario. */
export function aConversationRecord(
  overrides: Partial<ConversationRecordInit> = {},
): ConversationRecordInit {
  return {
    agent: "claude",
    id: "claude-main",
    model: "claude-opus-4-8",
    inputTokens: 0n,
    outputTokens: 0n,
    totalTokens: 0n,
    turns: 0,
    ...overrides,
  };
}

// ---------------------------------------------------------------------------
// Backend
// ---------------------------------------------------------------------------

/**
 * A `SessionsDrawerScreen` backend whose `TddyRemote.stream` yields one `tokenUsageUpdated`
 * `ServerMessage` per snapshot in `snapshots`. Pass `[]` for a session that never reports usage.
 */
export function aSessionUsageBackend(
  sessions: Partial<SessionEntry>[],
  snapshots: ConversationRecordInit[][],
): InMemoryRpcBackend {
  return aSessionsDrawerBackend(sessions).implement(TddyRemote, {
    stream: async function* () {
      for (const conversations of snapshots) {
        yield create(ServerMessageSchema, {
          event: { case: "tokenUsageUpdated", value: { conversations } },
        });
      }
    },
    getSession: async () => ({}),
    listSessions: async () => ({ sessions: [] }),
  });
}
