import React, { useState } from "react";
import { ConnectionService } from "../../gen/connection_pb";
import { safeTestIdPart } from "../../lib/testId";
import { useSelectedDaemon } from "../../rpc/selectedDaemon";
import { useHttpClient } from "../../rpc/transportProvider";
import { Button } from "../ui/button";
import { useAvailableAgents, type AvailableAgent } from "./useAvailableAgents";

/**
 * The fanned-out picker for a specialized agent to attach to a session: every common-room daemon's
 * offers under their qualified ids, with the tools the main agent would lose named before the attach
 * is confirmed.
 *
 * There is exactly one of these because there are two places to attach from — the Inspector's roster
 * pane and the session header — and an operator who is told two different things about the same
 * catalog has been told one wrong thing.
 *
 * Feature: docs/ft/daemon/session-agent-roster.md § Web UI (AC48-AC51);
 * Feature: docs/ft/web/session-drawer.md § Add agent.
 */

export interface AgentPickerProps {
  /**
   * Prefix for every test id this picker renders. **Explicit, with no default**: both mounts can be
   * on screen at once, and a shared default would make each of their selectors match the other's
   * controls. `agent-roster-picker` for the roster pane, `session-agent-picker` for the header.
   */
  readonly testIdPrefix: string;
  /**
   * Where a failed attach is rendered. Named separately from `testIdPrefix` because "the picker
   * could not be filled" and "the attach was refused" are different failures, and the surfaces
   * naming the second one do not all name it after their picker.
   */
  readonly errorTestId: string;
  /**
   * Attach the confirmed agent. Resolves to the message naming the failure, or `null` when the
   * attach went through — on which the picker closes itself through {@link onClose}.
   */
  readonly onAttach: (agent: AvailableAgent) => Promise<string | null>;
  /** Dismiss the picker, attaching nothing. */
  readonly onClose: () => void;
}

export function AgentPicker({ testIdPrefix, errorTestId, onAttach, onClose }: AgentPickerProps) {
  // The catalog is a fan-out, and its home is the host behind the browser's own transport — not the
  // host that owns the session. `ListSubagents` carries no routing field and a daemon never forwards
  // it, so the fan-out reads its home host through this client and addresses every *other*
  // common-room daemon over LiveKit RPC.
  const client = useHttpClient(ConnectionService);
  const { servingInstanceId } = useSelectedDaemon();
  const available = useAvailableAgents(client, servingInstanceId ?? "");

  const [pickedAgent, setPickedAgent] = useState<AvailableAgent | null>(null);
  const [attachError, setAttachError] = useState<string | null>(null);
  // True while an attach is on the wire. It closes the confirm control, because `onAttach` decides
  // whether the agent already has a conversation open from the state it can see *when it resolves*:
  // two attaches of the same agent in flight at once both find none, and the one that loses the race
  // leaves the surface focused on a conversation the other never kept.
  const [attaching, setAttaching] = useState(false);

  const optionTestId = (agentId: string) =>
    `${testIdPrefix}-option-${safeTestIdPart(agentId)}`;

  const confirm = async (agent: AvailableAgent) => {
    if (attaching) return;
    setAttachError(null);
    setAttaching(true);
    let failure: string | null;
    try {
      failure = await onAttach(agent);
    } finally {
      setAttaching(false);
    }
    if (failure === null) {
      onClose();
      return;
    }
    setAttachError(failure);
  };

  return (
    <div
      data-testid={testIdPrefix}
      className="flex flex-col gap-2 rounded-md border border-border bg-background p-2 text-xs"
    >
      {/* A host that could not answer costs its own row and nothing else — the other hosts'
          agents stay on offer. */}
      {available.failures.map((failure) => (
        <p
          key={failure.daemonInstanceId}
          data-testid={`${testIdPrefix}-host-error-${safeTestIdPart(failure.daemonInstanceId)}`}
          className="text-destructive"
        >
          {`${failure.daemonInstanceId}: ${failure.message}`}
        </p>
      ))}

      {available.agents.length === 0 && available.failures.length === 0 ? (
        <p className="text-muted-foreground">No agents on offer.</p>
      ) : (
        <ul className="flex flex-col gap-1">
          {available.agents.map((agent) => (
            <li key={agent.agentId}>
              <button
                type="button"
                data-testid={optionTestId(agent.agentId)}
                aria-pressed={pickedAgent?.agentId === agent.agentId}
                onClick={() => {
                  setPickedAgent(agent);
                  setAttachError(null);
                }}
                className={`flex w-full items-center justify-between gap-2 rounded-md border px-2 py-1 text-left ${
                  pickedAgent?.agentId === agent.agentId
                    ? "border-foreground"
                    : "border-transparent hover:border-border"
                }`}
              >
                <span className="truncate">{agent.label || agent.name}</span>
                <span
                  data-testid={`${optionTestId(agent.agentId)}-host`}
                  className="text-muted-foreground"
                >
                  {agent.daemonInstanceId}
                </span>
              </button>
            </li>
          ))}
        </ul>
      )}

      {/* The cost of the attach, stated before it is made: every tool named here stops being
          callable by the main agent for as long as this agent stays attached. */}
      {pickedAgent !== null && (
        <>
          <p
            data-testid={`${testIdPrefix}-withdrawal-warning`}
            className="text-muted-foreground"
          >
            {pickedAgent.replaces.length === 0
              ? `${pickedAgent.agentId} takes no tools away from the main agent.`
              : `The main agent loses ${pickedAgent.replaces.join(", ")} while ${pickedAgent.agentId} is attached.`}
          </p>
          {attachError !== null && (
            <p data-testid={errorTestId} className="text-destructive">
              {attachError}
            </p>
          )}
          <div className="flex gap-2">
            <Button
              data-testid={`${testIdPrefix}-confirm-btn`}
              size="sm"
              className="h-6 px-2 text-xs"
              // Reflects the gate in `confirm`, so the control says what it will do. It is not the
              // gate: a keyboard activation reaches `confirm` either way.
              disabled={attaching}
              onClick={() => void confirm(pickedAgent)}
            >
              Attach
            </Button>
            <Button
              data-testid={`${testIdPrefix}-cancel-btn`}
              variant="outline"
              size="sm"
              className="h-6 px-2 text-xs"
              onClick={onClose}
            >
              Cancel
            </Button>
          </div>
        </>
      )}
    </div>
  );
}
