import React from "react";
import type { SessionEntry } from "../../gen/connection_pb";
import { Button } from "../ui/button";

/**
 * The "Session agents" section — lists the selected session's peer agent sessions (children via
 * `orchestratorSessionId`), each with its agent/model/status and a "switch" action that focuses the
 * peer's runtime. Shows an empty state when there are no peers.
 *
 * PRD: docs/ft/web/1-WIP/PRD-2026-07-27-session-agent.md
 *
 * The `data-testid` values here mirror the constants in `cypress/support/testIds.ts`
 * (`sessionAgentsSection`, `sessionAgentsEmpty`, `sessionAgentsRow(<id>)`,
 * `sessionAgentsSwitchBtn(<id>)`).
 */

export interface SessionAgentsSectionProps {
  /** The peers of the current session (sessions with `orchestratorSessionId === currentSession`). */
  peers: ReadonlyArray<SessionEntry>;
  /** Fired when the operator clicks a peer's switch button — the parent focuses that peer's runtime
   *  (selects it in the drawer). */
  onSwitchPeer: (sessionId: string) => void;
}

/**
 * A short, human-readable status label for a peer row. Mirrors the drawer's status semantics:
 * "needs-input" when waiting for the operator, "active"/"idle"/"exited" otherwise.
 */
function peerStatusLabel(session: SessionEntry): string {
  if (session.pendingElicitation) return "needs-input";
  const status = (session.status ?? "").trim();
  return status === "" ? "idle" : status;
}

export function SessionAgentsSection({ peers, onSwitchPeer }: SessionAgentsSectionProps) {
  if (peers.length === 0) {
    return (
      <div
        data-testid="session-agents-empty"
        className="flex items-center justify-center px-3 py-2 text-xs text-muted-foreground"
      >
        No peer agents — click “Add agent” to spawn one.
      </div>
    );
  }

  return (
    <div
      data-testid="session-agents-section"
      className="flex flex-col gap-1 px-2 py-2 border-b border-border flex-shrink-0"
    >
      <div className="text-xs font-medium text-muted-foreground px-1 pb-1">
        Session agents ({peers.length})
      </div>
      <ul className="flex flex-col gap-1">
        {peers.map((peer) => (
          <li
            key={peer.sessionId}
            data-testid={`session-agents-row-${peer.sessionId}`}
            className="flex items-center justify-between gap-2 rounded-md border border-border bg-background px-2 py-1 text-xs"
          >
            <div className="flex flex-col min-w-0">
              <span className="truncate font-medium">
                {peer.agent || "agent"}
                {peer.model ? ` · ${peer.model}` : ""}
              </span>
              <span className="truncate text-muted-foreground">
                {peerStatusLabel(peer)}
              </span>
            </div>
            <Button
              data-testid={`session-agents-switch-btn-${peer.sessionId}`}
              variant="ghost"
              size="sm"
              className="h-6 px-2 text-xs"
              onClick={() => onSwitchPeer(peer.sessionId)}
              title={`Switch to peer ${peer.sessionId}`}
            >
              Switch
            </Button>
          </li>
        ))}
      </ul>
    </div>
  );
}
