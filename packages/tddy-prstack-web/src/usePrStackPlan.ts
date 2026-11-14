import { useEffect, useMemo, useState } from "react";
import type { Client } from "@connectrpc/connect";
import type { SessionEntry } from "tddy-client-web/gen/session_pb";
import type { PrStackService } from "tddy-client-web/gen/pr_stack_pb";
import { parseStackPlan } from "./stackPlan";
import { hydrateStackNodes } from "./hydrateStackNodes";
import { stackChildSessions, type StackChildMetadata } from "./stackChildSessions";
import { useQueryBranch } from "./useQueryBranch";
import { buildBranchQueries } from "./branchQueries";

/**
 * The PR-stack screen's plan loading (`#orchestrators` 12/12, B11): the session's stack addressed
 * by id, loaded from the store by `GetProjectStack`, hydrated with every child session on any
 * host, and each owned branch's one-call resolution. Split out of `PrStackScreen` as
 * `usePrStackPlan`.
 */
export function usePrStackPlan(input: {
  session: SessionEntry;
  prStackClient?: Client<typeof PrStackService>;
  sessionToken: string;
  sessions: SessionEntry[];
  sessionMetadataBySessionId: ReadonlyMap<string, StackChildMetadata>;
  defaultBranch: string;
}) {
  const { session, prStackClient, sessionToken, sessions, sessionMetadataBySessionId, defaultBranch } = input;

  // Overrides `session.stackPlanJson` immediately after a successful `AddPlannedPr`, since the
  // `session` prop itself only refreshes once the caller separately refetches the session list.
  // Reset whenever the prop actually changes so a later real refetch isn't masked by a stale one.
  const [stackPlanOverride, setStackPlanOverride] = useState<string | null>(null);
  useEffect(() => {
    setStackPlanOverride(null);
  }, [session.stackPlanJson]);
  const stack = useMemo(
    () => parseStackPlan(stackPlanOverride ?? session.stackPlanJson),
    [stackPlanOverride, session.stackPlanJson],
  );
  // The stack this session's changeset names, addressed by id (`#orchestrators` 9/11): every
  // planned-PR call names it when the session carries one, and the panel is loaded by it.
  const stackAddress = session.stackId
    ? {
        projectId: session.projectId,
        stackId: session.stackId,
        daemonInstanceId: session.daemonInstanceId,
      }
    : undefined;
  const stackTargetFields = stackAddress
    ? { stack: stackAddress }
    : { sessionId: session.sessionId };
  // Load the panel by the stack's id when the session names one, so the list is the store's
  // record rather than the list snapshot the session happened to carry.
  useEffect(() => {
    if (!prStackClient || !stackAddress) return;
    let cancelled = false;
    prStackClient
      .getProjectStack({ sessionToken, stack: stackAddress })
      .then((res) => {
        if (!cancelled && res.stackPlanJson) setStackPlanOverride(res.stackPlanJson);
      })
      .catch(() => {
        // The session's own snapshot stays on screen; a failed read must not blank the panel.
      });
    return () => {
      cancelled = true;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [prStackClient, sessionToken, session.stackId]);
  // Every session in this stack, on any host, in the one shape the view joins on (D38).
  const childSessions = useMemo(
    () => stackChildSessions(sessions, sessionMetadataBySessionId),
    [sessions, sessionMetadataBySessionId],
  );
  // The plan as it stands, plus the `branch` and `session_id` a cross-host spawn wrote on the child's
  // own disk and this orchestrator's `changeset.yaml` therefore never learned. Hydrated once, here,
  // so `nodes` is what the whole screen reads: the poll set, base resolution, the spawn gate, the
  // parent picker and each row's branch line all become correct without a case of their own.
  const nodes = useMemo(
    () => hydrateStackNodes(stack.nodes, childSessions, session.sessionId),
    [stack.nodes, childSessions, session.sessionId],
  );
  // What to ask `QueryBranch` to resolve: one call per branch a node owns, each paired with that
  // node's base so the daemon can also report how the two stand against each other. `QueryBranch`
  // resolves each branch's live GitHub PR itself, so the screen makes no second PR lookup:
  // `GetPrStatus` reaches the same authenticated `GET /pulls` on the daemon, and polling both would
  // double the GitHub request rate for no extra information — enough to exhaust a 5000/hour user
  // limit within the hour on a five-node stack, after which every row reads "PR status unavailable"
  // and stays there.
  const branchQueries = useMemo(
    () => buildBranchQueries(nodes, defaultBranch),
    [nodes, defaultBranch],
  );
  // One-call branch resolution (worktree + in-progress session + remote + PR + base sync) per branch,
  // polled on the same interval and independent of the agent.
  const { resolutionByBranch: branchResolutionByBranch, setResolution } = useQueryBranch(
    prStackClient,
    sessionToken,
    stackAddress ? { stack: stackAddress } : { sessionId: session.sessionId },
    branchQueries,
  );

  return {
    nodes,
    childSessions,
    stackAddress,
    stackTargetFields,
    branchResolutionByBranch,
    branchQueries,
    setResolution,
    setStackPlanOverride,
  };
}
