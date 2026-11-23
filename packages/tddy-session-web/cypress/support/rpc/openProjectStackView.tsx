/**
 * Open a work queue in the sessions drawer, over an in-memory backend: the inspector's **Work queue**
 * tab of the session that owns it (`#workqueue` 5/14 — a queue is its session's, there is no route of
 * its own).
 *
 * The planned-PR rows are the queue view's (`WorkQueueView`): the daemon answers `GetWorkQueue`
 * with the queue's plan, and the view acts on the queue by its address (its owning session's id). A
 * spec states the plan it wants shown and the sessions the drawer lists (the children a planned node
 * resolves to).
 */

import React from "react";
import { create } from "@bufbuild/protobuf";
import type { InMemoryRpcBackend } from "tddy-connectrpc-testkit";
import { WorkQueueService } from "tddy-client-web/gen/work_queue_pb";
import { SessionsDrawerScreen } from "../../../src/SessionsDrawerScreen";
import { sessionsServedBy } from "./vncBackend";
import { mountWithRpc } from "./inMemory";
import { withSelectedDaemon } from "./withSelectedDaemon";

/** The queue every spec opens: owned by the session `createdBySessionId` names in its call. */
export const THE_STACK = {
  projectId: "proj-pr-stack",
  queueId: "0190b000-0000-7000-8000-0000000000dd",
} as const;

/**
 * Make `backend` serve `THE_STACK` — `GetWorkQueue` answers with `plan`, naming
 * `createdBySessionId` as the session that created the stack (the parent a planned node's child
 * session is started under) — and point the app's location at its stack view. Mount the sessions
 * drawer after this to see it.
 */
export function serveProjectStack(
  backend: InMemoryRpcBackend,
  plan: string,
  createdBySessionId: string,
): InMemoryRpcBackend {
  // The session that owns the queue is among the drawer's sessions, owning it.
  const served = sessionsServedBy(backend);
  const owner = served.find((session) => session.sessionId === createdBySessionId);
  if (owner === undefined) {
    served.push({
      sessionId: createdBySessionId,
      projectId: THE_STACK.projectId,
      ownsWorkQueue: true,
    });
  } else {
    owner.ownsWorkQueue = true;
  }
  window.location.hash = `#/sessions/${createdBySessionId}?inspector=work-queue`;
  return backend
    .onUnary(WorkQueueService.method.listProjectWorkQueues, () =>
      create(WorkQueueService.method.listProjectWorkQueues.output, { stacks: [] }),
    )
    .onUnary(WorkQueueService.method.getWorkQueue, () =>
      create(WorkQueueService.method.getWorkQueue.output, {
        workQueueJson: plan,
        summary: { createdBySessionId },
      }),
    );
}

/** Serve `THE_STACK` over `backend` and mount the sessions drawer at its stack view. */
export function openProjectStackView(
  backend: InMemoryRpcBackend,
  plan: string,
  createdBySessionId: string,
): Cypress.Chainable {
  serveProjectStack(backend, plan, createdBySessionId);
  return mountWithRpc(withSelectedDaemon(<SessionsDrawerScreen />), backend);
}

/**
 * Open the stack view of the stack a spec describes as an orchestrator session: the stack's plan is
 * the `workQueueJson` of that session among the sessions `backend` (an `aSessionsDrawerBackend`)
 * lists, and the session is the one that created the stack.
 */
export function openProjectStackViewOf(
  backend: InMemoryRpcBackend,
  orchestratorSessionId: string,
): Cypress.Chainable {
  const orchestrator = sessionsServedBy(backend).find(
    (session) => session.sessionId === orchestratorSessionId,
  );
  if (orchestrator === undefined) {
    throw new Error(`no session ${orchestratorSessionId} among the drawer's sessions`);
  }
  // The session owns the queue the spec describes.
  orchestrator.ownsWorkQueue = true;
  return openProjectStackView(backend, orchestrator.workQueueJson ?? "", orchestratorSessionId);
}
