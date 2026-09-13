/**
 * In-memory `worktree.WorktreeService` backend — the session Worktree tab's cache-backed list and
 * its clear/delete/restore writes, plus the Worktrees screen's lazy streamed disk usage.
 *
 * Split out of `daemonSessionHostBackend.ts` when the nine worktree RPCs left
 * `session.SessionService` for `worktree.WorktreeService`: a fake is registered per service,
 * so a screen's backend composes the two rather than one `.implement` covering both.
 * `aWorktreeServiceFake` is the composable half (`handlers` spread into a caller's own
 * `.implement(WorktreeService, …)`), `aWorktreeServiceBackend` the standalone one, mirroring
 * `sessionAgentRosterBackend.ts`.
 */

import { create } from "@bufbuild/protobuf";
import type { ServiceImpl } from "@connectrpc/connect";
import { anInMemoryRpcBackend, type InMemoryRpcBackend } from "tddy-connectrpc-testkit";
import {
  CalculateWorktreeSizeResponseSchema,
  CleanWorktreeResponseSchema,
  ListWorktreesForProjectResponseSchema,
  RemoveWorktreeResponseSchema,
  RestoreSessionWorktreeResponseSchema,
  WorktreeRowSchema,
  WorktreeService,
  WorktreeSizeStatus,
  WorktreeStatsEventSchema,
  type WorktreeRow,
} from "../../../src/gen/worktree_pb";

/** One worktree row as fed into a `StreamWorktreeStats` snapshot/update frame. */
export interface WorktreeStatsRowInput {
  path: string;
  branchLabel?: string;
  /** On-disk size in bytes; meaningful once `sizeStatus` is `CACHED`. */
  diskBytes?: bigint;
  /** Lazy size lifecycle state (drives the Worktrees screen's Status cell). */
  sizeStatus: WorktreeSizeStatus;
  /** Unix epoch (ms) of the last size calculation; `0n`/omitted means "never". */
  sizeCalculatedAtUnixMs?: bigint;
  changedFiles?: number;
  linesAdded?: bigint;
  linesRemoved?: bigint;
  stale?: boolean;
}

export function aWorktreeRow(input: WorktreeStatsRowInput): WorktreeRow {
  return create(WorktreeRowSchema, {
    path: input.path,
    branchLabel: input.branchLabel ?? "",
    diskBytes: input.diskBytes ?? 0n,
    changedFiles: input.changedFiles ?? 0,
    linesAdded: input.linesAdded ?? 0n,
    linesRemoved: input.linesRemoved ?? 0n,
    stale: input.stale ?? false,
    sizeStatus: input.sizeStatus,
    sizeCalculatedAtUnixMs: input.sizeCalculatedAtUnixMs ?? 0n,
  });
}

export interface WorktreeServiceScenario {
  /** Rows returned by `ListWorktreesForProject` (Session Worktree tab). Default: none. */
  worktrees?: Array<{
    path: string;
    branchLabel?: string;
    diskBytes?: bigint;
    changedFiles?: number;
    linesAdded?: bigint;
    linesRemoved?: bigint;
    updatedAtUnixMs?: bigint;
    stale?: boolean;
  }>;
  /** First `StreamWorktreeStats` frame — the full snapshot of the project's worktrees. Default: none. */
  worktreeStatsSnapshot?: WorktreeStatsRowInput[];
  /** Optional second `StreamWorktreeStats` frame — one worktree whose size finished (Calculating → Cached). */
  worktreeStatsUpdate?: WorktreeStatsRowInput;
}

export interface WorktreeServiceControls {
  /** Number of `ListWorktreesForProject` calls with `refresh: true` — asserts the 10-min cadence. */
  readonly listWorktreesRefreshCount: () => number;
  /** Every `worktree_path` passed to `CleanWorktree`, in call order. */
  readonly cleanedWorktreePaths: string[];
  /** Every `worktree_path` passed to `RemoveWorktree`, in call order. */
  readonly removedWorktreePaths: string[];
  /** Every `session_id` passed to `RestoreSessionWorktree`, in call order. */
  readonly restoredSessionIds: string[];
  /** Number of times `StreamWorktreeStats` was subscribed (each open re-runs the async generator). */
  readonly worktreeStatsStreamCount: () => number;
  /** The `recalculate_all` flag of every `StreamWorktreeStats` subscription, in call order. */
  readonly worktreeStatsRecalculateAllFlags: boolean[];
  /** Every `worktree_path` passed to `CalculateWorktreeSize`, in call order. */
  readonly calculatedWorktreePaths: string[];
}

/** The worktree fake as handlers, so a screen's own backend can serve it too. */
export interface WorktreeServiceFake extends WorktreeServiceControls {
  handlers: Partial<ServiceImpl<typeof WorktreeService>>;
}

/**
 * Build the `worktree.WorktreeService` handlers for `scenario`, plus the recorders a spec asserts on.
 *
 * `StreamWorktreeStats` emits a first snapshot frame of every worktree, optionally one "updated"
 * frame for a worktree whose size flipped Calculating → Cached, then stays open (mirrors
 * `StreamHostStats` — a completed stream would read like the daemon dropping the feed).
 */
export function aWorktreeServiceFake(
  scenario: WorktreeServiceScenario = {},
): WorktreeServiceFake {
  let listWorktreesRefreshCalls = 0;
  const cleanedWorktreePaths: string[] = [];
  const removedWorktreePaths: string[] = [];
  const restoredSessionIds: string[] = [];
  let worktreeStatsStreamOpens = 0;
  const worktreeStatsRecalculateAllFlags: boolean[] = [];
  const calculatedWorktreePaths: string[] = [];

  const handlers: Partial<ServiceImpl<typeof WorktreeService>> = {
    listWorktreesForProject: async (req) => {
      if (req.refresh) listWorktreesRefreshCalls += 1;
      return create(ListWorktreesForProjectResponseSchema, {
        worktrees: (scenario.worktrees ?? []).map((w) => create(WorktreeRowSchema, w)),
      });
    },
    removeWorktree: async (req) => {
      removedWorktreePaths.push(req.worktreePath);
      return create(RemoveWorktreeResponseSchema, { ok: true, message: "" });
    },
    cleanWorktree: async (req) => {
      cleanedWorktreePaths.push(req.worktreePath);
      return create(CleanWorktreeResponseSchema, { ok: true, message: "" });
    },
    restoreSessionWorktree: async (req) => {
      restoredSessionIds.push(req.sessionId);
      return create(RestoreSessionWorktreeResponseSchema, {
        ok: true,
        message: "",
        worktreePath: `/restored/${req.sessionId}`,
      });
    },
    streamWorktreeStats: async function* (req) {
      worktreeStatsStreamOpens += 1;
      worktreeStatsRecalculateAllFlags.push(req.recalculateAll);
      yield create(WorktreeStatsEventSchema, {
        snapshot: (scenario.worktreeStatsSnapshot ?? []).map(aWorktreeRow),
      });
      if (scenario.worktreeStatsUpdate) {
        yield create(WorktreeStatsEventSchema, {
          updated: aWorktreeRow(scenario.worktreeStatsUpdate),
        });
      }
      await new Promise<never>(() => undefined);
    },
    calculateWorktreeSize: async (req) => {
      calculatedWorktreePaths.push(req.worktreePath);
      return create(CalculateWorktreeSizeResponseSchema, { ok: true, message: "" });
    },
  };

  return {
    handlers,
    listWorktreesRefreshCount: () => listWorktreesRefreshCalls,
    cleanedWorktreePaths,
    removedWorktreePaths,
    restoredSessionIds,
    worktreeStatsStreamCount: () => worktreeStatsStreamOpens,
    worktreeStatsRecalculateAllFlags,
    calculatedWorktreePaths,
  };
}

export interface WorktreeServiceBackend extends WorktreeServiceControls {
  backend: InMemoryRpcBackend;
}

/** The same fake as a backend of its own, for a spec that needs nothing but `worktree.WorktreeService`. */
export function aWorktreeServiceBackend(
  scenario: WorktreeServiceScenario = {},
): WorktreeServiceBackend {
  const { handlers, ...controls } = aWorktreeServiceFake(scenario);
  return {
    backend: anInMemoryRpcBackend().implement(WorktreeService, handlers),
    ...controls,
  };
}
