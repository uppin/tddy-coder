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
 * PRD: docs/ft/web/1-WIP/PRD-2026-10-03-indexing-indicators.md
 */

import React from "react";
import { Room } from "livekit-client";
import { createClient } from "@connectrpc/connect";
import {
  create,
  type DescMethodUnary,
  type MessageInitShape,
  type MessageShape,
} from "@bufbuild/protobuf";
import { anInMemoryRpcBackend, type InMemoryRpcBackend } from "tddy-connectrpc-testkit";
import { CatalogService } from "../../src/gen/catalog_pb";
import { CodeIndexProgressSchema, CodeNavigationService } from "../../src/gen/code_navigation_pb";
import { ProjectService } from "../../src/gen/project_pb";
import {
  SessionService,
  StartPhase_Boundary,
  StartPhase_Step,
  StartSessionEventSchema,
} from "../../src/gen/session_pb";
import { SessionFilesService } from "../../src/gen/session_files_pb";
import { WorktreeService } from "../../src/gen/worktree_pb";
import { CreateSessionPane } from "../../src/components/sessions/CreateSessionPane";
import { SessionIndexingIndicator } from "../../src/components/session/SessionIndexingIndicator";
import type { DaemonHost } from "../../src/lib/participantRole";
import { SelectedDaemonProvider } from "../../src/rpc/selectedDaemon";
import { mountWithRpc } from "../support/rpc/inMemory";
import { createSessionPage } from "../support/pages/createSessionPage";
import { sessionIndexingIndicatorPage } from "../support/pages/sessionIndexingIndicatorPage";

const LOCAL_HOST = "workstation-1";
const DAEMON_HOSTS: DaemonHost[] = [
  { instanceId: LOCAL_HOST, label: "workstation-1 (this daemon)" },
];
/** The Agent select's option value, qualified by the host that offers it. */
const CLAUDE_OPTION = `claude@${LOCAL_HOST}`;
const THE_SESSION = "session-indexing-1";

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
      answering(CatalogService.method.listAgents, { agents: [{ id: "claude", label: "Claude" }] }),
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
  mountWithRpc(
    <SelectedDaemonProvider room={new Room()} daemons={DAEMON_HOSTS} servingInstanceId={LOCAL_HOST}>
      <CreateSessionPane
        client={createClient(SessionService, transport)}
        projectClient={createClient(ProjectService, transport)}
        catalogClient={createClient(CatalogService, transport)}
        sessionFilesClient={createClient(SessionFilesService, transport)}
        worktreeClient={createClient(WorktreeService, transport)}
        sessionToken="fake-token"
        onCancel={cy.stub()}
        onCreated={cy.stub().as("onCreated")}
      />
    </SelectedDaemonProvider>,
    backend,
  );
}

/**
 * Today the form streams a start only when something is attached (a start with nothing attached
 * stays on the unary RPC, pinned by `CreateSessionAttachmentProgress.cy.tsx`), so the creation here
 * carries one attachment.
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

/**
 * A host whose warm of the session's code index reports 40% of its "Fetching" phase, holds until
 * `gate` is released, then reports the index ready and ends.
 */
function aHostWarmingTheSessionsIndex(gate: Gate): InMemoryRpcBackend {
  return anInMemoryRpcBackend().implement(CodeNavigationService, {
    async *watchCodeIndex() {
      yield create(CodeIndexProgressSchema, {
        line: "Fetching 40%",
        phase: "Fetching",
        percentage: 40,
        furthest: "40%",
      });
      await gate.held;
      yield create(CodeIndexProgressSchema, {
        line: "ready",
        phase: "Fetching",
        percentage: 100,
        furthest: "100%",
        ready: true,
      });
    },
  });
}

function mountTheIndexingIndicator(backend: InMemoryRpcBackend) {
  mountWithRpc(
    <SessionIndexingIndicator
      client={createClient(CodeNavigationService, backend.transport())}
      sessionToken="fake-token"
      sessionId={THE_SESSION}
    />,
    backend,
  );
}

beforeEach(() => {
  cy.viewport(1280, 900);
  cy.clearAllSessionStorage();
});

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

it("the create pane shows the current start phase", () => {
  // Given a host that is creating the session's worktree
  const gate = aGate();
  mountTheCreatePane(aHostHoldingTheStartInItsWorktreeStep(gate));

  // When a session is created
  createASessionWithOneAttachment();

  // Then the pane names the step the host is in while Create is disabled
  createSessionPage.startPhase().should("have.text", "Creating worktree…");
  createSessionPage.submitButton().should("be.disabled");

  // And the session is created once the host has started its agent and reported its result
  cy.then(() => gate.release());
  cy.get("@onCreated").should("have.been.calledWith", "phase-1");
});

it("the session header shows indexing until ready", () => {
  // Given a host warming the session's code index, 40% through its "Fetching" phase
  const gate = aGate();
  mountTheIndexingIndicator(aHostWarmingTheSessionsIndex(gate));

  // Then the header says the index is loading, with its phase and percentage
  sessionIndexingIndicatorPage.indicator().should("have.text", "Indexing — Fetching 40%");

  // When the index becomes ready
  cy.then(() => gate.release());

  // Then the indicator is gone
  sessionIndexingIndicatorPage.indicator().should("not.exist");
});
