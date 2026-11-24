/**
 * In-memory half of `session.SessionService` that closes a session — `PreviewCloseSession` and the
 * server-streaming `CloseSession` (docs/ft/web/session-drawer.md § Closing a session).
 *
 * A close is a *sequence* of events (started, steps, one terminal event), and the drawer's spinner
 * and its concurrency are only observable while a close is still running, so each close is
 * scripted: the events it streams at once, and optionally a hold that keeps the stream open until
 * the spec releases it.
 *
 * The handlers are exported separately from the controls for the same reason the roster and
 * conversation fakes' are: Connect's router fills every method a service implementation omits with
 * an `Unimplemented` handler, so the screen-level backend must spread them into its ONE
 * `.implement(SessionService, …)` call rather than registering the service twice.
 */

import { create, type MessageInitShape } from "@bufbuild/protobuf";
import type { ServiceImpl } from "@connectrpc/connect";
import {
  CloseSessionEventSchema,
  CloseSessionPreviewSchema,
  CloseSessionStepKind,
  CloseSessionStepState,
  CloseSessionWorktreeOwner,
  type CloseSessionEvent,
  type SessionService,
} from "tddy-client-web/gen/session_pb";

export type ClosePreviewInit = MessageInitShape<typeof CloseSessionPreviewSchema>;
export type CloseEventInit = MessageInitShape<typeof CloseSessionEventSchema>;
type CloseWorktreeInit = NonNullable<ClosePreviewInit["worktrees"]>[number];
type CloseStepInit = Extract<NonNullable<CloseEventInit["event"]>, { case: "step" }>["value"];

/** How one `CloseSession` call runs. */
export interface CloseScript {
  /** Streamed as soon as the close is requested. */
  events: CloseEventInit[];
  /**
   * Keep the stream open after `events` until {@link CloseSessionControls.releaseClose} is called
   * for the session, then stream `afterRelease` and end. The only way to observe what the drawer
   * does *while* a close runs.
   */
  holdOpen?: boolean;
  /** Streamed once a held close is released. */
  afterRelease?: CloseEventInit[];
}

export interface CloseSessionScenario {
  /**
   * What `PreviewCloseSession` answers, per session id. A list answers successive calls in order,
   * its last entry repeating (an unreadable preview, then a readable one on Retry). A session with
   * no entry has nothing at risk.
   */
  closePreviews?: Record<string, ClosePreviewInit | ClosePreviewInit[]>;
  /**
   * How `CloseSession` runs, per session id. A list scripts successive closes of the session in
   * order, its last entry repeating. A session with no entry is closed at once: `started`, then an
   * `ok` outcome.
   */
  closeScripts?: Record<string, CloseScript | CloseScript[]>;
}

/** One `CloseSession` request, as the drawer sent it. */
export interface RecordedCloseRequest {
  readonly sessionId: string;
  readonly confirmed: boolean;
  readonly confirmedFingerprint: string;
}

export interface CloseSessionControls {
  /** The `session_id` of every `PreviewCloseSession`, in call order. */
  previewCloseCalls: () => string[];
  /** Every `CloseSession` request, in call order — recorded when the request arrives, so a held
   *  close is already in the list while it runs. */
  closeRequests: () => RecordedCloseRequest[];
  /**
   * Let a held close of `sessionId` finish. Throws when no close of that session is being held, so
   * a spec cannot release a close that never started and still pass.
   */
  releaseClose: (sessionId: string) => void;
}

type CloseHandlers = Pick<ServiceImpl<typeof SessionService>, "previewCloseSession" | "closeSession">;

/** The answer for the `call`-th request (0-based) out of a configured single value or list. */
function nth<T>(configured: T | T[] | undefined, call: number): T | undefined {
  if (configured === undefined) return undefined;
  if (!Array.isArray(configured)) return configured;
  return configured[Math.min(call, configured.length - 1)];
}

export function aCloseSessionFake(
  scenario: CloseSessionScenario,
): { handlers: CloseHandlers } & CloseSessionControls {
  const previewCalls: string[] = [];
  const requests: RecordedCloseRequest[] = [];
  const held = new Map<string, () => void>();

  const handlers: CloseHandlers = {
    previewCloseSession: async (req) => {
      const call = previewCalls.filter((id) => id === req.sessionId).length;
      previewCalls.push(req.sessionId);
      return create(
        CloseSessionPreviewSchema,
        nth(scenario.closePreviews?.[req.sessionId], call) ?? aClosePreviewWithNothingAtRisk(),
      );
    },
    closeSession: async function* (req): AsyncGenerator<CloseSessionEvent> {
      const call = requests.filter((r) => r.sessionId === req.sessionId).length;
      requests.push({
        sessionId: req.sessionId,
        confirmed: req.confirmed,
        confirmedFingerprint: req.confirmedFingerprint,
      });
      const script = nth(scenario.closeScripts?.[req.sessionId], call) ?? aCloseThatSucceeds();
      for (const event of script.events) yield create(CloseSessionEventSchema, event);
      if (!script.holdOpen) return;
      await new Promise<void>((resolve) => held.set(req.sessionId, resolve));
      for (const event of script.afterRelease ?? []) yield create(CloseSessionEventSchema, event);
    },
  };

  return {
    handlers,
    previewCloseCalls: () => [...previewCalls],
    closeRequests: () => [...requests],
    releaseClose: (sessionId: string) => {
      const release = held.get(sessionId);
      if (release === undefined) {
        throw new Error(`releaseClose: no close of ${sessionId} is being held open.`);
      }
      held.delete(sessionId);
      release();
    },
  };
}

// ---------------------------------------------------------------------------
// Preview builders
// ---------------------------------------------------------------------------

/** A preview of a session whose worktree is clean and pushed: nothing is asked. */
export function aClosePreviewWithNothingAtRisk({
  fingerprint = "sha256:e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
}: { fingerprint?: string } = {}): ClosePreviewInit {
  return {
    worktrees: [],
    tabs: [{ tabId: "main", label: "claude (opus)" }],
    fingerprint,
    needsConfirmation: false,
  };
}

/** The session's own worktree, on this daemon's host, with nothing to lose unless overridden. */
export function aSessionWorktree(overrides: Partial<CloseWorktreeInit> = {}): CloseWorktreeInit {
  return {
    path: "/home/dev/.tddy/worktrees/tddy-coder/feature-session-close",
    host: "",
    owner: CloseSessionWorktreeOwner.SESSION,
    ownerId: "",
    branch: "feature/session-close-and-remove",
    dirtyFiles: [],
    unpushedCommits: [],
    unpushedCommitCount: 0,
    unavailableReason: "",
    ...overrides,
  };
}

/** A clone-placed subagent conversation's worktree under the session's own. */
export function aConversationWorktree(
  conversationId: string,
  overrides: Partial<CloseWorktreeInit> = {},
): CloseWorktreeInit {
  return aSessionWorktree({
    path: `/home/dev/.tddy/worktrees/tddy-coder/feature-session-close/tmp/subagent-worktrees/${conversationId}`,
    owner: CloseSessionWorktreeOwner.CONVERSATION,
    ownerId: conversationId,
    branch: `subagent/${conversationId}`,
    ...overrides,
  });
}

// ---------------------------------------------------------------------------
// Event builders
// ---------------------------------------------------------------------------

export function closeStarted(taskId: string, daemonInstanceId = "local"): CloseEventInit {
  return { event: { case: "started", value: { taskId, daemonInstanceId } } };
}

export function closeStep(step: CloseStepInit): CloseEventInit {
  return { event: { case: "step", value: step } };
}

/** Terminal: the daemon found more at risk than was confirmed, and did nothing further. */
export function closeAsksAgain(preview: ClosePreviewInit): CloseEventInit {
  return { event: { case: "needsConfirmation", value: preview } };
}

export function closeOutcome(failedSteps: CloseStepInit[] = []): CloseEventInit {
  return { event: { case: "outcome", value: { ok: failedSteps.length === 0, failedSteps } } };
}

// ---------------------------------------------------------------------------
// Close scripts
// ---------------------------------------------------------------------------

/** A close that starts and ends `ok` at once. */
export function aCloseThatSucceeds(taskId = "close-task-0001"): CloseScript {
  return { events: [closeStarted(taskId), closeOutcome()] };
}

/** A close that starts, reports it is terminating the main tab, and runs until released. */
export function aCloseThatRunsUntilReleased(taskId = "close-task-0002"): CloseScript {
  return {
    events: [
      closeStarted(taskId),
      closeStep({
        kind: CloseSessionStepKind.TERMINATE,
        subject: "main",
        state: CloseSessionStepState.RUNNING,
        message: "terminating claude (opus)",
        host: "",
      }),
    ],
    holdOpen: true,
    afterRelease: [closeOutcome()],
  };
}

/** A close that ends with `failedSteps`. */
export function aCloseThatFails(failedSteps: CloseStepInit[], taskId = "close-task-0003"): CloseScript {
  return {
    events: [closeStarted(taskId), ...failedSteps.map(closeStep), closeOutcome(failedSteps)],
  };
}

/** A close the daemon stops at its check, asking again with `preview`. */
export function aCloseThatAsksAgain(preview: ClosePreviewInit, taskId = "close-task-0004"): CloseScript {
  return { events: [closeStarted(taskId), closeAsksAgain(preview)] };
}
