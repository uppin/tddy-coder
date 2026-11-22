/**
 * Open a project's stack view in the sessions drawer by its route, over an in-memory backend.
 *
 * The planned-PR rows are the stack view's (`ProjectStackView`): the daemon answers `GetProjectStack`
 * with the stack's plan, and the view acts on the stack by its address. A spec states the plan it
 * wants shown and the sessions the drawer lists (the children a planned node resolves to); there is
 * no orchestrator session behind the stack.
 */

import React from "react";
import { create } from "@bufbuild/protobuf";
import type { InMemoryRpcBackend } from "tddy-connectrpc-testkit";
import { PrStackService } from "tddy-client-web/gen/pr_stack_pb";
import { SessionsDrawerScreen } from "../../../src/SessionsDrawerScreen";
import { sessionsServedBy } from "./vncBackend";
import { mountWithRpc } from "./inMemory";
import { withSelectedDaemon } from "./withSelectedDaemon";

/** The stack every spec opens. */
export const THE_STACK = {
  projectId: "proj-pr-stack",
  stackId: "0190b000-0000-7000-8000-0000000000dd",
} as const;

/**
 * Make `backend` serve `THE_STACK` — `GetProjectStack` answers with `plan`, naming
 * `createdBySessionId` as the session that created the stack (the parent a planned node's child
 * session is started under) — and point the app's location at its stack view. Mount the sessions
 * drawer after this to see it.
 */
export function serveProjectStack(
  backend: InMemoryRpcBackend,
  plan: string,
  createdBySessionId: string,
): InMemoryRpcBackend {
  window.location.hash = `#/sessions/project/${THE_STACK.projectId}/stack/${THE_STACK.stackId}`;
  return backend
    .onUnary(PrStackService.method.listProjectStacks, () =>
      create(PrStackService.method.listProjectStacks.output, { stacks: [] }),
    )
    .onUnary(PrStackService.method.getProjectStack, () =>
      create(PrStackService.method.getProjectStack.output, {
        stackPlanJson: plan,
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
 * the `stackPlanJson` of that session among the sessions `backend` (an `aSessionsDrawerBackend`)
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
  return openProjectStackView(backend, orchestrator.stackPlanJson ?? "", orchestratorSessionId);
}
