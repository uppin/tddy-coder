/**
 * In-memory backend + fixtures for the Session Inspector "Usage" tab acceptance tests.
 *
 * Token usage is served by the daemon's `ActivityService.StreamSessionUsage`: the full cumulative
 * per-conversation snapshot, then each changed snapshot (`#usage-binding 2/5`). These helpers drive
 * that stream through `anInMemoryRpcBackend()` + `mountWithRpc` — the fluent-tests-preferred
 * in-memory fake. No LiveKit room is involved: the tab reaches the daemon the way the Activity tab
 * does.
 *
 * Each element of `snapshots` is one full cumulative snapshot (a list of conversations); the
 * stream yields one `SessionUsageSnapshot` per element, in order, then closes — so the view renders
 * the latest snapshot.
 */

import { create } from "@bufbuild/protobuf";
import type { InMemoryRpcBackend } from "tddy-connectrpc-testkit";
import type { SessionEntry } from "tddy-client-web/gen/session_pb";
import { ActivityService, SessionUsageSnapshotSchema } from "tddy-client-web/gen/activity_pb";
import { aSessionsDrawerBackend } from "./vncBackend";

// ---------------------------------------------------------------------------
// UsageConversation fixture
// ---------------------------------------------------------------------------

/**
 * Wire shape of one conversation's accounting (matches proto `UsageConversation`; 64-bit token
 * counts are `bigint`, `turns` is a 32-bit `number`).
 */
export interface ConversationRecordInit {
  agent: string;
  id: string;
  model: string;
  inputTokens: bigint;
  cacheReadTokens: bigint;
  cacheWriteTokens: bigint;
  outputTokens: bigint;
  reasoningTokens: bigint;
  totalTokens: bigint;
  turns: number;
  costUsd?: number;
  parentId?: string;
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
    cacheReadTokens: 0n,
    cacheWriteTokens: 0n,
    outputTokens: 0n,
    reasoningTokens: 0n,
    totalTokens: 0n,
    turns: 0,
    ...overrides,
  };
}

// ---------------------------------------------------------------------------
// Backend
// ---------------------------------------------------------------------------

/**
 * A `SessionsDrawerScreen` backend whose `ActivityService.streamSessionUsage` yields one snapshot
 * per element of `snapshots`. Pass `[]` for a session that never reports usage.
 */
export function aSessionUsageBackend(
  sessions: Partial<SessionEntry>[],
  snapshots: ConversationRecordInit[][],
): InMemoryRpcBackend {
  return aSessionsDrawerBackend(sessions).implement(ActivityService, {
    streamSessionUsage: async function* () {
      for (const conversations of snapshots) {
        yield create(SessionUsageSnapshotSchema, { conversations });
      }
    },
  });
}
