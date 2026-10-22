import { useEffect, useMemo, useState } from "react";
import type { SessionEntry } from "../../gen/session_pb";
import { prStackOrchestrators, stackBaseSessionCandidates } from "../../utils/stackParents";

export interface UseCreateSessionStackArgs {
  /** The whole session list as the daemon reported it. */
  sessions: SessionEntry[];
  projectId: string;
  daemonInstanceId: string;
  /** The caller's pre-filled stack parent, if any. */
  initialStackParent: string | undefined;
  /** The planned node the caller opened this form for, if any. */
  initialStackNodeId: string | undefined;
}

/**
 * The new session's place in a PR stack: the orchestrator it is parented to (and the host that owns
 * it), the planned node it was opened for, and the session whose branch seeds a new orchestrator's
 * stack. Lifted out of `CreateSessionPane.tsx` with its state and its reset effect.
 */
export function useCreateSessionStack({
  sessions,
  projectId,
  daemonInstanceId,
  initialStackParent,
  initialStackNodeId,
}: UseCreateSessionStackArgs) {
  const [stackParent, setStackParent] = useState(initialStackParent ?? "");
  // The planned node the caller opened this form for. Not state of its own: the operator picks a
  // stack parent here, never a node — so the id is derived from whether the parent is still the one
  // the node belongs to, and dropped the moment it is not.
  //
  // A node id names a node in exactly one orchestrator's plan. Sent beside a different parent, the
  // daemon's `LinkStackNode` answers `not_found` and — per D36, so a failed link never fails a spawn
  // that already created the worktree — only logs it: the node would stay branchless and childless,
  // which is the very bug the id exists to fix, with nothing said to the operator. An empty id falls
  // the daemon back to its branch-derived local lookup, which is correct for a same-host spawn and is
  // what every spawn used before the id existed (D34).
  const stackNodeId = stackParent === (initialStackParent ?? "") ? (initialStackNodeId ?? "") : "";
  // The existing session whose branch seeds a new pr-stack orchestrator's stack as its single root
  // node. Empty leaves the stack unseeded, which is what every caller sent before this control
  // existed — the agent then plans it.
  const [prStackBaseSessionId, setPrStackBaseSessionId] = useState("");

  // The orchestrators this session can be stack-parented to.
  const stackParentOptions = useMemo(() => prStackOrchestrators(sessions), [sessions]);

  // The host that owns the selected stack parent. `stackParent` is a bare session id and the base
  // it resolves to is read out of that session's own changeset, so the daemon serving this start
  // has to be told which host holds it: the picker is fed by a fanned-out, all-host `ListSessions`
  // and routinely offers an orchestrator living on a different daemon than the child is started on.
  // Empty when the list holds no such session — the wire default, meaning the serving daemon's own
  // sessions tree, which is the only place it could look anyway.
  const stackParentDaemonInstanceId = useMemo(
    () => sessions.find((s) => s.sessionId === stackParent)?.daemonInstanceId ?? "",
    [sessions, stackParent],
  );

  // The sessions whose branch can seed this orchestrator's stack — scoped to the project and host
  // the form will actually create it on, because a base session in another repository (or on another
  // daemon's checkout) owns a branch this stack cannot base anything off. Derived here rather than
  // beside the parent picker above because it depends on the project/host the form currently holds.
  const stackBaseSessionOptions = useMemo(
    () => stackBaseSessionCandidates(sessions, { projectId, daemonInstanceId }),
    [sessions, projectId, daemonInstanceId],
  );

  // A base session belongs to one project on one host, so switching either abandons the choice. Reset
  // it *visibly* — the control returns to "None (agent plans the stack)", which is what submit would
  // now do — instead of leaving a value the picker no longer offers selected behind a blank <select>.
  useEffect(() => {
    setPrStackBaseSessionId("");
  }, [projectId, daemonInstanceId]);

  return {
    stackParent,
    setStackParent,
    stackNodeId,
    prStackBaseSessionId,
    setPrStackBaseSessionId,
    stackParentOptions,
    stackParentDaemonInstanceId,
    stackBaseSessionOptions,
  };
}
