/**
 * Acceptance: while a session is created, the create pane lists the start's steps under the host
 * doing the work, each in progress, done or failed, and shows the clone's progress — git's stage, a
 * percent bar, bytes received and throughput — while the clone step runs.
 *
 * Mid-stream state is asserted while the stub generator is held at a gate, so the rendered value is
 * exact rather than whatever the race settled on (the `SessionStartAndIndexingProgress.cy.tsx`
 * technique).
 *
 * Feature docs: docs/ft/web/1-WIP/PRD-2026-10-24-start-progress-progress-ui.md
 */

import React from "react";
import { Room } from "livekit-client";
import { Code, ConnectError, createClient } from "@connectrpc/connect";
import {
  create,
  type DescMethodUnary,
  type MessageInitShape,
  type MessageShape,
} from "@bufbuild/protobuf";
import { anInMemoryRpcBackend, type InMemoryRpcBackend } from "tddy-connectrpc-testkit";
import { CatalogService } from "../../src/gen/catalog_pb";
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
import type { DaemonHost } from "../../src/lib/participantRole";
import { SelectedDaemonProvider } from "../../src/rpc/selectedDaemon";
import { mountWithRpc } from "../support/rpc/inMemory";
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

function aCloneProgress(stage: string, percent: number, bytesReceived: number, bytesPerSecond: number) {
  return create(StartSessionEventSchema, {
    event: {
      case: "cloneProgress",
      value: {
        stage,
        percent,
        objectsDone: BigInt(percent),
        objectsTotal: 100n,
        bytesReceived: BigInt(bytesReceived),
        bytesPerSecond: BigInt(bytesPerSecond),
        line: `${stage}: ${percent}%`,
      },
    },
  });
}

function aResult() {
  return create(StartSessionEventSchema, {
    event: { case: "result", value: { sessionId: "progress-1" } },
  });
}

const MIB = 1024 * 1024;

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

function createASession() {
  createSessionPage.selectProject("proj-1");
  createSessionPage.selectAgent(CLAUDE_OPTION);
  createSessionPage.submit();
}

describe("Create session — start progress", () => {
  it("lists the steps of the start in order, ticking each as it ends", () => {
    // Given a host that clones, then holds in its worktree step
    const cloning = aGate();
    const cutting = aGate();
    mountTheCreatePane(
      aBaselineBackend().implement(SessionService, {
        async *streamStartSession() {
          yield aPhase(StartPhase_Step.CLONE, StartPhase_Boundary.BEGIN);
          await cloning.held;
          yield aPhase(StartPhase_Step.CLONE, StartPhase_Boundary.END);
          yield aPhase(StartPhase_Step.WORKTREE, StartPhase_Boundary.BEGIN);
          await cutting.held;
          yield aPhase(StartPhase_Step.WORKTREE, StartPhase_Boundary.END);
          yield aResult();
        },
      }),
    );

    // When
    createASession();

    // Then — the clone is in progress
    createSessionPage.startStep("clone").should("have.attr", "data-state", "active");
    createSessionPage.startStep("worktree").should("not.exist");

    // And once the clone ends, it is ticked and the worktree is in progress
    cy.then(() => cloning.release());
    createSessionPage.startStep("clone").should("have.attr", "data-state", "done");
    createSessionPage.startStep("worktree").should("have.attr", "data-state", "active");
    cy.then(() => cutting.release());
    cy.get("@onCreated").should("have.been.calledWith", "progress-1");
  });

  it("shows the clone's stage, percent, bytes and throughput, updating in place", () => {
    // Given a host receiving objects
    const first = aGate();
    const second = aGate();
    mountTheCreatePane(
      aBaselineBackend().implement(SessionService, {
        async *streamStartSession() {
          yield aPhase(StartPhase_Step.CLONE, StartPhase_Boundary.BEGIN);
          yield aCloneProgress("Receiving objects", 45, 38.2 * MIB, 4.1 * MIB);
          await first.held;
          yield aCloneProgress("Receiving objects", 60, 51 * MIB, 4.1 * MIB);
          await second.held;
          yield aPhase(StartPhase_Step.CLONE, StartPhase_Boundary.END);
          yield aResult();
        },
      }),
    );

    // When
    createASession();

    // Then
    createSessionPage
      .cloneProgress()
      .should("contain.text", "Receiving objects")
      .and("contain.text", "45%")
      .and("contain.text", "38.2 MiB")
      .and("contain.text", "4.1 MiB/s");
    createSessionPage.cloneProgress().find("[role='progressbar']").should("have.attr", "aria-valuenow", "45");

    // And the next report replaces it rather than adding a line
    cy.then(() => first.release());
    createSessionPage.cloneProgress().should("have.length", 1).and("contain.text", "60%").and("contain.text", "51 MiB");
    cy.then(() => second.release());
  });

  it("shows no clone row for a start that does not clone", () => {
    // Given a host whose project is already present
    const gate = aGate();
    mountTheCreatePane(
      aBaselineBackend().implement(SessionService, {
        async *streamStartSession() {
          yield aPhase(StartPhase_Step.WORKTREE, StartPhase_Boundary.BEGIN);
          await gate.held;
          yield aPhase(StartPhase_Step.WORKTREE, StartPhase_Boundary.END);
          yield aResult();
        },
      }),
    );

    // When
    createASession();

    // Then
    createSessionPage.startStep("worktree").should("have.attr", "data-state", "active");
    createSessionPage.startStep("clone").should("not.exist");
    createSessionPage.cloneProgress().should("not.exist");
    cy.then(() => gate.release());
  });

  it("shows the step a failed start stopped at, with the error", () => {
    // Given a host whose worktree step fails
    mountTheCreatePane(
      aBaselineBackend().implement(SessionService, {
        async *streamStartSession() {
          yield aPhase(StartPhase_Step.WORKTREE, StartPhase_Boundary.BEGIN);
          throw new ConnectError("worktree setup failed: base ref not found", Code.Internal);
        },
      }),
    );

    // When
    createASession();

    // Then
    createSessionPage.startStep("worktree").should("have.attr", "data-state", "failed");
    createSessionPage.error().should("contain.text", "worktree setup failed: base ref not found");
  });

  it("names the host doing the work", () => {
    // Given
    const gate = aGate();
    mountTheCreatePane(
      aBaselineBackend().implement(SessionService, {
        async *streamStartSession() {
          yield aPhase(StartPhase_Step.WORKTREE, StartPhase_Boundary.BEGIN);
          await gate.held;
          yield aResult();
        },
      }),
    );

    // When
    createASession();

    // Then
    createSessionPage.startProgressHeading().should("contain.text", "Creating session on workstation-1");
    cy.then(() => gate.release());
  });
});
