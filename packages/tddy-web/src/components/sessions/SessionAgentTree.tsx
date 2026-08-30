import React, { useState } from "react";
import type { Client } from "@connectrpc/connect";
import {
  AgentCloneState,
  type ConnectionService,
  type SessionAgentActivity,
  type SessionAgentEntry,
  type SessionAgentStatus,
  type SessionEntry,
} from "../../gen/connection_pb";
import { safeTestIdPart } from "../../lib/testId";
import { Button } from "../ui/button";
import {
  agentStatusName,
  agentStatusToken,
  lastActivityText,
  statusIsWorking,
} from "./agentStatusDisplay";
import type { SubagentSessionNode } from "./agentTree";
import { rosterHalfOf, type RosterHalf } from "./sessionRosterHalf";
import { useSessionAgentRoster } from "./useSessionAgentRoster";

/**
 * The Agents tab as a **tree**: the session's own main agent at the root, and beneath it the two
 * populations of agents working for it — the **managed** roster agents whose loop the facilitating
 * daemon runs (`StreamSessionAgents`), and the **non-managed** subagent *sessions* it spawned, each
 * nesting its own roster agents and its own subagents in turn.
 *
 * Both kinds carry the same badge from the same vocabulary (`agentStatusDisplay`), because the proto
 * ships one `SessionAgentStatus` for both: a managed row reads `SessionAgentEntry.status`, a
 * non-managed one the inferred `SessionEntry.agent_status`. What a row *is* is stated in
 * `data-agent-kind` rather than left to be guessed from its label — the two afford different
 * actions.
 *
 * PRD: docs/ft/web/1-WIP/PRD-2026-08-30-agents-tab-subagent-tree.md
 */

// ---------------------------------------------------------------------------
// Test ids — mirrored in `cypress/support/testIds.ts` (`agentRoster*`, `agentTree*`).
// ---------------------------------------------------------------------------

const rosterRowTestId = (agentId: string) => `agent-roster-row-${safeTestIdPart(agentId)}`;
const sessionRowTestId = (sessionId: string) => `agent-tree-session-${safeTestIdPart(sessionId)}`;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/**
 * The clone state as a word, for both the badge's text and its `data-clone-state`. `local` is a
 * state of its own and not a synonym for `ready`: there is no checkout behind a local agent, so
 * saying "ready" would imply one exists.
 */
const CLONE_STATE_NAMES: Record<AgentCloneState, string> = {
  [AgentCloneState.UNSPECIFIED]: "unspecified",
  [AgentCloneState.LOCAL]: "local",
  [AgentCloneState.PROVISIONING]: "provisioning",
  [AgentCloneState.READY]: "ready",
  [AgentCloneState.ERROR]: "error",
};

/** A value this build has no name for is shown as itself, not folded into a state it is not. */
function cloneStateName(state: AgentCloneState): string {
  return CLONE_STATE_NAMES[state] ?? String(state);
}

// ---------------------------------------------------------------------------
// The two pieces every row shares
// ---------------------------------------------------------------------------

/**
 * What an agent is doing.
 *
 * One component for all three rows, because the proto ships one `SessionAgentStatus` for a managed
 * roster agent and a non-managed subagent session alike. Sharing the vocabulary but not the markup
 * would still let a main-agent badge and a subagent badge drift apart in emphasis or in the
 * attribute a selector reads.
 *
 * Always rendered, including for UNSPECIFIED: a row with no status badge and a row whose daemon has
 * nothing to say look identical otherwise, and only one of them is a build that forgot to send the
 * field.
 */
function AgentStatusBadge({ testId, status }: { testId: string; status: SessionAgentStatus }) {
  return (
    <span className="flex flex-wrap items-center gap-1 text-muted-foreground">
      <span
        data-testid={testId}
        data-agent-status={agentStatusToken(status)}
        className={
          statusIsWorking(status) ? "font-medium text-foreground" : "text-muted-foreground"
        }
      >
        {agentStatusName(status)}
      </span>
    </span>
  );
}

/**
 * The last thing this row was observed doing, or nothing at all when it has been observed doing
 * nothing — an empty line reserved for a history that does not exist is a row that looks like it
 * lost one.
 */
function LastActivityLine({
  testId,
  activity,
  now,
}: {
  testId: string;
  activity: SessionAgentActivity | undefined;
  now: number;
}) {
  if (activity === undefined) return null;
  return (
    <span
      data-testid={testId}
      className="truncate text-muted-foreground"
      title={activity.summary}
    >
      {lastActivityText(activity.summary, activity.atUnixMs, now)}
    </span>
  );
}

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/**
 * A detach an operator asked for, with everything the decision needs: the roster the entry belongs
 * to, because "the last agent of a remote daemon" is a fact about *that* roster, and the half it is
 * held on, because that is where the call has to be addressed.
 */
export interface RosterDetachRequest {
  readonly entry: SessionAgentEntry;
  readonly roster: ReadonlyArray<SessionAgentEntry>;
  readonly half: RosterHalf;
}

/** What every row in the tree needs to render itself and to reach the daemon holding its roster. */
interface TreeContext {
  readonly client: Client<typeof ConnectionService>;
  readonly sessionToken: string;
  /** Whether the daemon behind this tree is reachable. A collapsed-open node respects it too. */
  readonly daemonConnected: boolean;
  /** The pane's ticked clock, so every last-activity line in the tree ages together. */
  readonly now: number;
  readonly onSwitchSubagent: (sessionId: string) => void;
  readonly onDetach: (request: RosterDetachRequest) => void;
}

export interface SessionAgentTreeProps extends TreeContext {
  /** The session the tab is about — its own main agent is the root. */
  readonly session: SessionEntry;
  /** The roster attached to `session`, as the pane's stream last reported it. */
  readonly agents: ReadonlyArray<SessionAgentEntry>;
  /** The sessions `session` spawned, already folded by {@link subagentSessionNodes}. */
  readonly subagents: ReadonlyArray<SubagentSessionNode>;
}

// ---------------------------------------------------------------------------
// Component
// ---------------------------------------------------------------------------

export function SessionAgentTree({
  session,
  agents,
  subagents,
  ...context
}: SessionAgentTreeProps) {
  const half = rosterHalfOf(session);
  return (
    <ul data-testid="agent-tree" className="flex flex-col gap-1">
      <li
        data-testid="agent-tree-root"
        data-agent-kind="main"
        data-depth={0}
        className="flex flex-col gap-1 rounded-md border border-border bg-background px-2 py-1"
      >
        <div className="flex min-w-0 flex-col">
          <span className="truncate font-medium">{session.agent || session.sessionId}</span>
          <span className="truncate text-muted-foreground">{session.model}</span>
          <AgentStatusBadge testId="agent-tree-root-status" status={session.agentStatus} />
          <LastActivityLine
            testId="agent-tree-root-last-activity"
            activity={session.lastActivity}
            now={context.now}
          />
        </div>
        <ChildRows
          depth={1}
          agents={agents}
          half={half}
          subagents={subagents}
          testId="agent-tree-root-children"
          {...context}
        />
      </li>
    </ul>
  );
}

// ---------------------------------------------------------------------------
// The children of one node — its roster agents, then the sessions it spawned
// ---------------------------------------------------------------------------

interface ChildRowsProps extends TreeContext {
  readonly testId: string;
  readonly depth: number;
  readonly agents: ReadonlyArray<SessionAgentEntry>;
  /** Where `agents` is held, so a detach from this list is addressed to the right daemon. */
  readonly half: RosterHalf;
  readonly subagents: ReadonlyArray<SubagentSessionNode>;
}

/** Roster agents in attach order, then subagent sessions in list order (AC6). */
function ChildRows({ testId, depth, agents, half, subagents, ...context }: ChildRowsProps) {
  return (
    <ul data-testid={testId} className="flex flex-col gap-1 pl-3">
      {agents.map((entry) => (
        <RosterAgentRow
          key={entry.agentId}
          entry={entry}
          roster={agents}
          half={half}
          depth={depth}
          {...context}
        />
      ))}
      {subagents.map((node) => (
        <SubagentSessionRow
          key={node.session.sessionId}
          node={node}
          depth={depth}
          {...context}
        />
      ))}
    </ul>
  );
}

// ---------------------------------------------------------------------------
// A managed roster agent
// ---------------------------------------------------------------------------

interface RosterAgentRowProps extends TreeContext {
  readonly entry: SessionAgentEntry;
  readonly roster: ReadonlyArray<SessionAgentEntry>;
  readonly half: RosterHalf;
  readonly depth: number;
}

function RosterAgentRow({ entry, roster, half, depth, now, onDetach }: RosterAgentRowProps) {
  const testId = rosterRowTestId(entry.agentId);
  return (
    <li
      data-testid={testId}
      data-agent-kind="roster"
      data-depth={depth}
      className="flex items-center justify-between gap-2 rounded-md border border-border bg-background px-2 py-1"
    >
      <div className="flex min-w-0 flex-col">
        <span className="truncate font-medium">{entry.label || entry.agentId}</span>
        <span className="truncate text-muted-foreground">
          {`${entry.agentId}${entry.model ? ` · ${entry.model}` : ""}`}
        </span>
        <span className="flex flex-wrap items-center gap-1 text-muted-foreground">
          <span data-testid={`${testId}-host`}>{entry.daemonInstanceId}</span>
          <span
            data-testid={`${testId}-clone-state`}
            data-clone-state={cloneStateName(entry.cloneState)}
            title={entry.cloneError}
          >
            {cloneStateName(entry.cloneState)}
          </span>
        </span>
        <AgentStatusBadge testId={`${testId}-status`} status={entry.status} />
        <LastActivityLine
          testId={`${testId}-last-activity`}
          activity={entry.lastActivity}
          now={now}
        />
        {/* What the main agent lost to this row — the reason an operator would detach it. */}
        <span data-testid={`${testId}-replaces`} className="truncate text-muted-foreground">
          {entry.replaces.length === 0
            ? "takes no tools from the main agent"
            : `replaces ${entry.replaces.join(", ")}`}
        </span>
      </div>
      <Button
        data-testid={`${testId}-detach-btn`}
        variant="ghost"
        size="sm"
        className="h-6 px-2 text-xs"
        onClick={() => onDetach({ entry, roster, half })}
      >
        Detach
      </Button>
    </li>
  );
}

// ---------------------------------------------------------------------------
// A non-managed subagent session
// ---------------------------------------------------------------------------

interface SubagentSessionRowProps extends TreeContext {
  readonly node: SubagentSessionNode;
  readonly depth: number;
}

/**
 * A session the agent above it spawned, with its own roster and its own subagents beneath it.
 *
 * It is a component rather than a branch of a loop because it holds a **subscription**:
 * `useSessionAgentRoster` is per-session and hooks cannot be called in a loop, so each session node
 * subscribes for itself. That is also what makes "a collapsed node costs nothing" a fact rather than
 * a flag — an unmounted subscription is an unopened stream. A subagent's *own* status rides
 * `ListSessions`, so a collapsed row still says what it is doing.
 *
 * Its roster is read on its **own** codebase half: a subagent can be split independently of its
 * parent, and reading the agent half would return an empty list beside the real one.
 */
function SubagentSessionRow({ node, depth, ...context }: SubagentSessionRowProps) {
  const [expanded, setExpanded] = useState(false);
  const half = rosterHalfOf(node.session);
  const { agents, error } = useSessionAgentRoster({
    client: context.client,
    sessionToken: context.sessionToken,
    sessionId: half.sessionId,
    daemonInstanceId: half.daemonInstanceId,
    enabled: context.daemonConnected && expanded,
  });

  const session = node.session;
  const testId = sessionRowTestId(session.sessionId);
  return (
    <li
      data-testid={testId}
      data-agent-kind="session"
      data-depth={depth}
      className="flex flex-col gap-1 rounded-md border border-border bg-background px-2 py-1"
    >
      <div className="flex items-center justify-between gap-2">
        <div className="flex min-w-0 flex-col">
          <span className="truncate font-medium">{session.agent || session.sessionId}</span>
          <span className="truncate text-muted-foreground">
            {`${session.sessionId}${session.model ? ` · ${session.model}` : ""}`}
          </span>
          <AgentStatusBadge testId={`${testId}-status`} status={session.agentStatus} />
          <LastActivityLine
            testId={`${testId}-last-activity`}
            activity={session.lastActivity}
            now={context.now}
          />
        </div>
        <div className="flex flex-shrink-0 items-center gap-1">
          <Button
            data-testid={`${testId}-toggle-btn`}
            variant="ghost"
            size="sm"
            className="h-6 px-2 text-xs"
            aria-expanded={expanded}
            onClick={() => setExpanded((open) => !open)}
          >
            {expanded ? "Hide" : "Show"}
          </Button>
          {/* No Detach: there is no roster entry behind a subagent session, so a detach would have
              nothing to send. */}
          <Button
            data-testid={`${testId}-switch-btn`}
            variant="ghost"
            size="sm"
            className="h-6 px-2 text-xs"
            onClick={() => context.onSwitchSubagent(session.sessionId)}
          >
            Switch
          </Button>
        </div>
      </div>
      {expanded && (
        <>
          {/* A roster this node could not read is not a node with no agents. The pane refuses that
              conflation for the session it is about; a subagent's own host can be unreachable while
              the rest of the tree reads fine, so the reason is said on the row that has it. */}
          {error !== null && (
            <p
              data-testid={`${testId}-roster-error`}
              className="pl-3 text-destructive"
            >
              {error}
            </p>
          )}
          <ChildRows
            testId={`${testId}-children`}
            depth={depth + 1}
            agents={agents}
            half={half}
            subagents={node.children}
            {...context}
          />
        </>
      )}
    </li>
  );
}
