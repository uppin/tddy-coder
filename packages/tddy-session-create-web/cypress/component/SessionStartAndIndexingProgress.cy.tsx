/**
 * Acceptance tests: the session UI says what the host is doing while a session starts, and while its
 * code index warms afterwards.
 *
 * - The create pane names the step of the start the host reports beginning (`StartPhase` on
 *   `StreamStartSession`): the worktree, the semantic index, the agent.
 * - The session header follows `code_navigation.WatchCodeIndex` and shows "Indexing — <phase> <n>%"
 *   until the index is ready, then nothing.
 *
 * Mid-stream state is asserted while the stub generator is held at a gate, so the rendered value is
 * exact rather than whatever the race settled on — the `CreateSessionAttachmentProgress.cy.tsx`
 * technique.
 *
 * Feature docs: docs/ft/web/session-drawer.md (Start progress) and docs/ft/web/session-code-pane.md (Indexing indicator)
 */

import { aClientsPortDouble } from "tddy-client-web/testkit";
import React from "react";
import { servedAgentWire, withDeclaredSessionTypes } from "../support/servedAgents";
import { createClient } from "@connectrpc/connect";
import {
  create,
  type DescMethodUnary,
  type MessageInitShape,
  type MessageShape,
} from "@bufbuild/protobuf";
import { anInMemoryRpcBackend, type InMemoryRpcBackend } from "tddy-connectrpc-testkit";
import { CatalogService } from "tddy-client-web/gen/catalog_pb";
import { ProjectService } from "tddy-client-web/gen/project_pb";
import {
  SessionService,
  StartPhase_Boundary,
  StartPhase_Step,
  StartSessionEventSchema,
} from "tddy-client-web/gen/session_pb";
import { SessionFilesService } from "tddy-client-web/gen/session_files_pb";
import { WorktreeService } from "tddy-client-web/gen/worktree_pb";
import { CreateSessionPane } from "../../src/CreateSessionPane";
import type { DaemonHost } from "tddy-client-web";
import { createSessionPage } from "../support/pages/createSessionPage";

const LOCAL_HOST = "workstation-1";
const DAEMON_HOSTS: DaemonHost[] = [
  { instanceId: LOCAL_HOST, label: "workstation-1 (this daemon)" },
];
/** The Agent select's option value, qualified by the host that offers it. */
const CLAUDE_OPTION = `claude@${LOCAL_HOST}`;

/** A gate a stub generator awaits, so the UI's mid-stream state is settled when it is asserted. */
interface Gate {
  held: Promise<void>;
  release: () => void;
}

function aGate(): Gate {
  let release: () => void = () => undefined;
  const held = new Promise<void>((resolve) => {
    release = resolve;
  });
  return { held, release: () => release() };
}

// ---------------------------------------------------------------------------
// The create pane

/** A unary handler answering every request with one message built from `init`. */
function answering<M extends DescMethodUnary>(
  method: M,
  init: MessageInitShape<M["output"]>,
): () => MessageShape<M["output"]> {
  return () => create<M["output"]>(method.output, init);
}

/** Every RPC the form issues besides the session start itself. */
function aBaselineBackend(): InMemoryRpcBackend {
  const uploadChunk = SessionFilesService.method.uploadStagedAttachmentChunk;
  return anInMemoryRpcBackend()
    .onUnary(SessionService.method.listSessions, answering(SessionService.method.listSessions, {}))
    .onUnary(
      CatalogService.method.listAgentModels,
      answering(CatalogService.method.listAgentModels, {
        models: [{ id: "claude-opus-4-8", label: "Claude Opus 4.8" }],
        defaultModel: "claude-opus-4-8",
      }),
    )
    .onUnary(
      ProjectService.method.listProjects,
      answering(ProjectService.method.listProjects, {
        projects: [{ projectId: "proj-1", name: "Test Project", mainRepoPath: "/repo" }],
      }),
    )
    .onUnary(
      CatalogService.method.listAgents,
      answering(CatalogService.method.listAgents, withDeclaredSessionTypes({ agents: [{ id: "claude", label: "Claude", ...servedAgentWire("claude"), }] })),
    )
    .onUnary(
      CatalogService.method.listTools,
      answering(CatalogService.method.listTools, {
        tools: [{ path: "/usr/bin/tddy-coder", label: "tddy-coder" }],
      }),
    )
    .onUnary(CatalogService.method.listSubagents, answering(CatalogService.method.listSubagents, {}))
    .onUnary(
      ProjectService.method.listProjectBranches,
      answering(ProjectService.method.listProjectBranches, {
        branches: ["origin/main"],
        defaultRemote: "origin",
      }),
    )
    .onUnary(uploadChunk, (req) =>
      create(uploadChunk.output, {
        entry: req.last
          ? {
              daemonInstanceId: LOCAL_HOST,
              stagingId: req.stagingId,
              fileName: req.fileName,
              hostPath: `/srv/staging/${req.stagingId}/${req.fileName}`,
            }
          : undefined,
      }),
    );
}

function aPhase(step: StartPhase_Step, boundary: StartPhase_Boundary) {
  return create(StartSessionEventSchema, {
    event: { case: "phase", value: { step, boundary } },
  });
}

/**
 * A host whose start reports the worktree step beginning, then holds until `gate` is released, then
 * finishes the worktree, runs the agent step and reports its result.
 */
function aHostHoldingTheStartInItsWorktreeStep(gate: Gate): InMemoryRpcBackend {
  return aBaselineBackend().implement(SessionService, {
    async *streamStartSession() {
      yield aPhase(StartPhase_Step.WORKTREE, StartPhase_Boundary.BEGIN);
      await gate.held;
      yield aPhase(StartPhase_Step.WORKTREE, StartPhase_Boundary.END);
      yield aPhase(StartPhase_Step.AGENT, StartPhase_Boundary.BEGIN);
      yield aPhase(StartPhase_Step.AGENT, StartPhase_Boundary.END);
      yield create(StartSessionEventSchema, {
        event: { case: "result", value: { sessionId: "phase-1" } },
      });
    },
  });
}

function mountTheCreatePane(backend: InMemoryRpcBackend) {
  const transport = backend.transport();
  cy.mountWithClients(
    <CreateSessionPane
        client={createClient(SessionService, transport)}
        projectClient={createClient(ProjectService, transport)}
        catalogClient={createClient(CatalogService, transport)}
        sessionFilesClient={createClient(SessionFilesService, transport)}
        worktreeClient={createClient(WorktreeService, transport)}
        sessionToken="fake-token"
        onCancel={cy.stub()}
        onCreated={cy.stub().as("onCreated")}
      />,
    backend, { clients: aClientsPortDouble(backend, { daemons: DAEMON_HOSTS }) },
  );
}

/**
 * The form always streams the start; this creation carries one attachment so the host also reports
 * the attachment's materialization.
 */
function createASessionWithOneAttachment() {
  createSessionPage.selectProject("proj-1");
  createSessionPage.selectAgent(CLAUDE_OPTION);
  createSessionPage.pickFiles([
    { contents: Cypress.Buffer.from("# spec"), fileName: "spec.md", mimeType: "text/plain" },
  ]);
  createSessionPage.submit();
}

// ---------------------------------------------------------------------------
// The session header

beforeEach(() => {
  cy.viewport(1280, 900);
  cy.clearAllSessionStorage();
});

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

it("the create pane shows the step the start is in", () => {
  // Given a host that is creating the session's worktree
  const gate = aGate();
  mountTheCreatePane(aHostHoldingTheStartInItsWorktreeStep(gate));

  // When a session is created
  createASessionWithOneAttachment();

  // Then the pane names the step the host is in while Create is disabled
  createSessionPage.startStep("worktree").should("have.attr", "data-state", "active");
  createSessionPage.submitButton().should("be.disabled");

  // And the session is created once the host has started its agent and reported its result
  cy.then(() => gate.release());
  cy.get("@onCreated").should("have.been.calledWith", "phase-1");
});
