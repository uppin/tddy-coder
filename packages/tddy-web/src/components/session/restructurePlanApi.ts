import type { Client } from "@connectrpc/connect";
import {
  PlanOperationStatus,
  type CodeNavigationService,
  type PlanOperation,
  type PlanRunEvent,
} from "../../gen/code_navigation_pb";
import type { WorktreeFilesApiConfig } from "./worktreeFilesApi";

/** Where an operation's journal says it is. */
export type PlanOperationStatusName =
  "pending" | "in_flight" | "applied" | "failed" | "rolled_back";

/** One operation of a restructure plan, as the plan dialog shows it. */
export type PlanOperationRow = {
  /** The operation's stable id in its plan — rows, run events and staleness are keyed by it. */
  id: string;
  index: number;
  /** The operation kind as the plan spells it, e.g. `rename_symbol`. */
  op: string;
  /** The item or symbol path the anchor names; empty for a range anchor. */
  item: string;
  file: string;
  /** The transactional group; empty when the operation stands alone. */
  group: string;
  status: PlanOperationStatusName;
  /** Why the operation can no longer run as written, or `null` while it still points at its code. */
  staleReason: string | null;
};

/** One thing that happened while a plan ran. */
export type PlanRunUpdate =
  | { kind: "applied"; opId: string; done: number; total: number }
  | { kind: "note"; text: string }
  | { kind: "finished"; applied: number; total: number }
  | { kind: "failed"; message: string; group: string; rolledBack: string[] };

/**
 * Thin data-access adapter over the plan calls of `code_navigation.CodeNavigationService`, in the
 * shape of `CodeNavigationApi`: the session token, project id and worktree path are bound once, so
 * the dialog speaks only the plan's worktree-relative path.
 */
export interface RestructurePlanApi {
  /** Load the plan into the index's store and list its operations. */
  open(relPath: string): Promise<PlanOperationRow[]>;
  /** The plan's operations, again whenever a status or a stale reason changes. */
  watch(relPath: string, signal: AbortSignal): AsyncIterable<PlanOperationRow[]>;
  /** Apply the plan, yielding each operation's outcome as it lands, then the run's. */
  run(relPath: string, signal: AbortSignal): AsyncIterable<PlanRunUpdate>;
}

export type RestructurePlanApiConfig = WorktreeFilesApiConfig;

const STATUS_NAMES: Record<PlanOperationStatus, PlanOperationStatusName> = {
  [PlanOperationStatus.UNSPECIFIED]: "pending",
  [PlanOperationStatus.PENDING]: "pending",
  [PlanOperationStatus.IN_FLIGHT]: "in_flight",
  [PlanOperationStatus.APPLIED]: "applied",
  [PlanOperationStatus.FAILED]: "failed",
  [PlanOperationStatus.ROLLED_BACK]: "rolled_back",
};

function toRow(operation: PlanOperation): PlanOperationRow {
  return {
    id: operation.id,
    index: operation.index,
    op: operation.op,
    item: operation.item,
    file: operation.file,
    group: operation.group,
    status: STATUS_NAMES[operation.status],
    staleReason: operation.staleReason === "" ? null : operation.staleReason,
  };
}

function toUpdate(event: PlanRunEvent): PlanRunUpdate | null {
  switch (event.event.case) {
    case "operation":
      return {
        kind: "applied",
        opId: event.event.value.opId,
        done: event.event.value.done,
        total: event.event.value.total,
      };
    case "note":
      return { kind: "note", text: event.event.value };
    case "outcome":
      return {
        kind: "finished",
        applied: event.event.value.applied,
        total: event.event.value.total,
      };
    case "failure":
      return {
        kind: "failed",
        message: event.event.value.message,
        group: event.event.value.group,
        rolledBack: event.event.value.rolledBack,
      };
    default:
      return null;
  }
}

export function createRestructurePlanApi(
  client: Client<typeof CodeNavigationService>,
  { sessionToken, projectId, worktreePath }: RestructurePlanApiConfig,
): RestructurePlanApi {
  const request = (relPath: string) => ({
    sessionToken,
    projectId,
    worktreePath,
    relPath,
  });
  return {
    async open(relPath) {
      const snapshot = await client.openPlan(request(relPath));
      return snapshot.operations.map(toRow);
    },
    async *watch(relPath, signal) {
      for await (const snapshot of client.watchPlan(request(relPath), {
        signal,
      })) {
        yield snapshot.operations.map(toRow);
      }
    },
    async *run(relPath, signal) {
      for await (const event of client.runPlan(request(relPath), { signal })) {
        const update = toUpdate(event);
        if (update !== null) yield update;
      }
    },
  };
}

/**
 * Whether a previewed file is a restructure plan: a `.jsonl` whose first line is a plan header — a
 * JSON object carrying a numeric `v` and either a `snapshot` object (schema v1) or a `files` object
 * (schema v2), as `tddy-code-restructuring`'s `Plan::parse` reads it. An event log, whose lines are
 * not headers, is never offered as a plan.
 */
export function isRestructurePlanFile(relPath: string, content: string): boolean {
  if (!relPath.endsWith(".jsonl")) return false;
  const firstLine = content.split("\n").find((line) => line.trim() !== "");
  if (firstLine === undefined) return false;
  let header: unknown;
  try {
    header = JSON.parse(firstLine);
  } catch {
    return false;
  }
  if (typeof header !== "object" || header === null) return false;
  const { v, snapshot, files } = header as Record<string, unknown>;
  const isObject = (value: unknown) =>
    typeof value === "object" && value !== null && !Array.isArray(value);
  return typeof v === "number" && (isObject(snapshot) || isObject(files));
}
