/**
 * Acceptance tests: a restructure plan opened from the session Code pane.
 *
 * - Previewing a `.jsonl` whose first line is a restructure plan header offers "Open as plan"; any
 *   other `.jsonl` does not.
 * - The plan dialog lists every operation with its id, kind, item, group and status
 *   (`code_navigation.OpenPlan`), marks the ones that no longer point at their code with the plan
 *   store's reason (`WatchPlan`), and while any is stale disables Run and names it.
 * - Run (`RunPlan`) turns each operation's row applied as its event arrives.
 *
 * Mid-run state is asserted while the stub generator is held at a gate, so the rendered value is
 * exact rather than whatever the race settled on — the `SessionStartAndIndexingProgress.cy.tsx`
 * technique. All RPC flows through the in-memory backend — no HTTP intercepts.
 *
 * PRD: docs/ft/web/1-WIP/PRD-2026-10-03-plan-dialog.md
 */

import React from "react";
import { createClient } from "@connectrpc/connect";
import { create } from "@bufbuild/protobuf";
import { anInMemoryRpcBackend, type InMemoryRpcBackend } from "tddy-connectrpc-testkit";
import {
  CodeNavigationService,
  PlanOperationAppliedSchema,
  PlanOperationSchema,
  PlanOperationStatus,
  PlanRunEventSchema,
  PlanRunOutcomeSchema,
  PlanSnapshotSchema,
  type PlanSnapshot,
  type RunPlanRequest,
} from "../../src/gen/code_navigation_pb";
import {
  ListWorktreeDirectoryResponseSchema,
  ReadWorktreeFileResponseSchema,
  WorktreeDirEntrySchema,
  WorktreeService,
} from "../../src/gen/worktree_pb";
import { WorktreeCodePane } from "../../src/components/session/WorktreeCodePane";
import { RestructurePlanDialog } from "../../src/components/session/RestructurePlanDialog";
import { createRestructurePlanApi } from "../../src/components/session/restructurePlanApi";
import { mountWithRpc } from "../support/rpc/inMemory";
import { requestFields } from "../support/rpc/recordedRequests";
import { restructurePlanDialogPage } from "../support/pages/restructurePlanDialogPage";
import { worktreeCodePanePage } from "../support/pages/worktreeCodePanePage";

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

const SESSION_TOKEN = "fake-token";
const PROJECT_ID = "proj-plan-dialog";
const WORKTREE_PATH = "/home/dev/plan-dialog-project";
const WORKTREE = {
  sessionToken: SESSION_TOKEN,
  projectId: PROJECT_ID,
  worktreePath: WORKTREE_PATH,
};

const THE_PLAN = "plans/split-geometry.jsonl";
const THE_PLAN_JSONL = [
  '{"v":1,"snapshot":{}}',
  '{"id":"op-rename","op":"rename_symbol","anchor":{"kind":"symbol","file":"src/main.rs","path":"foo"},"name":"area"}',
  '{"id":"op-move","op":"move_symbol","anchor":{"kind":"symbol","file":"src/main.rs","path":"area"},"to":"src/geometry.rs","group":"geometry"}',
  "",
].join("\n");

/** A `.jsonl` that is not a plan: an event log whose first line carries no plan header. */
const AN_EVENT_LOG = "logs/events.jsonl";
const AN_EVENT_LOG_JSONL = ['{"event":"started","at":"2026-10-03T09:00:00Z"}', ""].join("\n");

/** The plan's two operations: a rename standing alone, then a move in the group `geometry`. */
function thePlanSnapshot({ opMoveStaleReason = "" } = {}): PlanSnapshot {
  return create(PlanSnapshotSchema, {
    relPath: THE_PLAN,
    operations: [
      create(PlanOperationSchema, {
        id: "op-rename",
        index: 0,
        op: "rename_symbol",
        item: "foo",
        file: "src/main.rs",
        group: "",
        status: PlanOperationStatus.PENDING,
        staleReason: "",
      }),
      create(PlanOperationSchema, {
        id: "op-move",
        index: 1,
        op: "move_symbol",
        item: "area",
        file: "src/main.rs",
        group: "geometry",
        status: PlanOperationStatus.PENDING,
        staleReason: opMoveStaleReason,
      }),
    ],
  });
}

function anOperationApplied(opId: string, index: number, done: number) {
  return create(PlanRunEventSchema, {
    event: {
      case: "operation",
      value: create(PlanOperationAppliedSchema, {
        opId,
        index,
        done,
        total: 2,
        files: ["src/main.rs"],
      }),
    },
  });
}

/** A gate a stub generator awaits, so the dialog's mid-run state is settled when it is asserted. */
interface Gate {
  held: Promise<void>;
  release: () => void;
}

function aGate(): Gate {
  let release = () => {};
  const held = new Promise<void>((resolve) => {
    release = resolve;
  });
  return { held, release };
}

// ---------------------------------------------------------------------------
// Backends
// ---------------------------------------------------------------------------

/** A worktree holding the plan and an event log, whose host opens the plan with nothing stale. */
function aWorktreeHoldingThePlan(): InMemoryRpcBackend {
  const directories: Record<string, Array<{ name: string; isDir: boolean }>> = {
    "": [
      { name: "logs", isDir: true },
      { name: "plans", isDir: true },
    ],
    logs: [{ name: "events.jsonl", isDir: false }],
    plans: [{ name: "split-geometry.jsonl", isDir: false }],
  };
  const files: Record<string, string> = {
    [THE_PLAN]: THE_PLAN_JSONL,
    [AN_EVENT_LOG]: AN_EVENT_LOG_JSONL,
  };
  return aHostHoldingThePlan()
    .implement(WorktreeService, {
      listWorktreeDirectory: (req) =>
        create(ListWorktreeDirectoryResponseSchema, {
          entries: (directories[req.relPath] ?? []).map((e) => create(WorktreeDirEntrySchema, e)),
        }),
      readWorktreeFile: (req) =>
        create(ReadWorktreeFileResponseSchema, {
          contentUtf8: files[req.relPath] ?? "",
          truncated: false,
          byteSize: BigInt((files[req.relPath] ?? "").length),
        }),
    });
}

/** A host whose plan store holds the plan with nothing stale, and whose watch says so once. */
function aHostHoldingThePlan(): InMemoryRpcBackend {
  return anInMemoryRpcBackend().implement(CodeNavigationService, {
    openPlan: () => thePlanSnapshot(),
    async *watchPlan() {
      yield thePlanSnapshot();
    },
  });
}

/** A host that opens the plan with nothing stale, then reports its move's item changed. */
function aHostWhereTheMoveGoesStale(): InMemoryRpcBackend {
  return anInMemoryRpcBackend().implement(CodeNavigationService, {
    openPlan: () => thePlanSnapshot(),
    async *watchPlan() {
      yield thePlanSnapshot({ opMoveStaleReason: "item changed" });
    },
  });
}

/**
 * A host whose run of the plan lands the rename, holds until `gate` is released, then lands the
 * move and finishes. Each run request is appended to `runs`.
 */
function aHostRunningThePlan(gate: Gate, runs: RunPlanRequest[]): InMemoryRpcBackend {
  return anInMemoryRpcBackend().implement(CodeNavigationService, {
    openPlan: () => thePlanSnapshot(),
    async *watchPlan() {
      yield thePlanSnapshot();
    },
    async *runPlan(req) {
      runs.push(req);
      yield anOperationApplied("op-rename", 0, 1);
      await gate.held;
      yield anOperationApplied("op-move", 1, 2);
      yield create(PlanRunEventSchema, {
        event: {
          case: "outcome",
          value: create(PlanRunOutcomeSchema, { applied: 2, total: 2 }),
        },
      });
    },
  });
}

// ---------------------------------------------------------------------------
// Mounting
// ---------------------------------------------------------------------------

/** Mount the Code pane over `backend` and preview `relPath`, which sits one directory down. */
function previewInTheCodePane(backend: InMemoryRpcBackend, relPath: string) {
  const transport = backend.transport();
  mountWithRpc(
    <WorktreeCodePane
      client={createClient(WorktreeService, transport)}
      navigationClient={createClient(CodeNavigationService, transport)}
      {...WORKTREE}
    />,
    backend,
  );
  worktreeCodePanePage.node(relPath.split("/")[0]).click();
  worktreeCodePanePage.node(relPath).click();
}

/** Mount the plan dialog for the plan over `backend`. */
function openThePlanDialog(backend: InMemoryRpcBackend) {
  const api = createRestructurePlanApi(
    createClient(CodeNavigationService, backend.transport()),
    WORKTREE,
  );
  mountWithRpc(<RestructurePlanDialog api={api} relPath={THE_PLAN} onClose={() => {}} />, backend);
}

beforeEach(() => {
  cy.viewport(1280, 800);
});

// ---------------------------------------------------------------------------
// The Code pane entry
// ---------------------------------------------------------------------------

it("opening a plan file shows open as plan", () => {
  // Given
  previewInTheCodePane(aWorktreeHoldingThePlan(), THE_PLAN);

  // When
  restructurePlanDialogPage.openAsPlan().should("be.visible").click();

  // Then — the dialog opens on that plan.
  restructurePlanDialogPage.dialog().should("be.visible");
  restructurePlanDialogPage.title().should("have.text", THE_PLAN);
});

it("another jsonl file shows no plan entry", () => {
  // Given + When
  previewInTheCodePane(aWorktreeHoldingThePlan(), AN_EVENT_LOG);

  // Then — once the log is previewed, nothing offers to open it as a plan.
  worktreeCodePanePage.preview().should("contain.text", '"event":"started"');
  restructurePlanDialogPage.openAsPlan().should("not.exist");
});

// ---------------------------------------------------------------------------
// The dialog
// ---------------------------------------------------------------------------

it("the dialog lists operations with status and group", () => {
  // Given + When
  openThePlanDialog(aHostHoldingThePlan());

  // Then
  restructurePlanDialogPage.op("op-rename").should("have.text", "rename_symbol");
  restructurePlanDialogPage.item("op-rename").should("have.text", "foo (src/main.rs)");
  restructurePlanDialogPage.group("op-rename").should("have.text", "");
  restructurePlanDialogPage.status("op-rename").should("have.text", "pending");
  restructurePlanDialogPage.op("op-move").should("have.text", "move_symbol");
  restructurePlanDialogPage.item("op-move").should("have.text", "area (src/main.rs)");
  restructurePlanDialogPage.group("op-move").should("have.text", "geometry");
  restructurePlanDialogPage.status("op-move").should("have.text", "pending");
  restructurePlanDialogPage.run().should("be.enabled");
});

it("a stale operation disables run and names it", () => {
  // Given + When — the watch reports the move's item changed after the plan opened.
  openThePlanDialog(aHostWhereTheMoveGoesStale());

  // Then
  restructurePlanDialogPage.stale("op-move").should("have.text", "stale — item changed");
  restructurePlanDialogPage.stale("op-rename").should("have.text", "");
  restructurePlanDialogPage.staleNotice().should("have.text", "op-move: stale — item changed");
  restructurePlanDialogPage.run().should("be.disabled");
});

it("running turns each row applied as its event arrives", () => {
  // Given
  const gate = aGate();
  const runs: RunPlanRequest[] = [];
  openThePlanDialog(aHostRunningThePlan(gate, runs));
  restructurePlanDialogPage.status("op-move").should("have.text", "pending");

  // When
  restructurePlanDialogPage.run().click();

  // Then — the rename's event has arrived and the move's has not.
  restructurePlanDialogPage.status("op-rename").should("have.text", "applied");
  restructurePlanDialogPage.status("op-move").should("have.text", "pending");
  cy.wrap(null).should(() => {
    expect(runs.map((run) => requestFields(run))).to.deep.equal([
      { ...WORKTREE, relPath: THE_PLAN },
    ]);
  });

  // When — the move lands.
  cy.then(() => gate.release());

  // Then
  restructurePlanDialogPage.status("op-move").should("have.text", "applied");
});
