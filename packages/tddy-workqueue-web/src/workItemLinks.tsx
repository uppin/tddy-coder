import { createContext, useContext } from "react";
import type { WorkItemCheck } from "tddy-client-web/gen/work_queue_pb";

/** One session a work item's link control can offer — the part of a `SessionEntry` it needs. */
export interface LinkableSession {
  sessionId: string;
  /** What the operator reads in the picker: the session's label or, failing that, its id. */
  label: string;
  daemonInstanceId: string;
}

/** An item's PR checks at its head, as the queue's status stream reports them: counts and each run. */
export interface ItemChecks {
  running: number;
  failed: number;
  succeeded: number;
  runs: WorkItemCheck[];
}

/**
 * What a row needs to link a PR or a session to its item (`#workqueue` 6/14). Supplied by the view
 * that owns the queue's clients and read by each expanded row, so the row list does not carry four
 * more callbacks through every layer. Absent (no client) means the controls are not offered.
 */
export interface WorkItemLinks {
  sessions: LinkableSession[];
  checksByItemId: Readonly<Record<string, ItemChecks>>;
  /** The reason the last link call of an item was refused, shown inline. */
  errorByItemId: Readonly<Record<string, string>>;
  onAssignPr: (itemId: string, pullNumber: number) => void;
  onUnassignPr: (itemId: string) => void;
  onLinkSession: (itemId: string, session: LinkableSession) => void;
  onUnlinkSession: (itemId: string, sessionId: string) => void;
}

export const WorkItemLinksContext = createContext<WorkItemLinks | null>(null);

export function useWorkItemLinks(): WorkItemLinks | null {
  return useContext(WorkItemLinksContext);
}
