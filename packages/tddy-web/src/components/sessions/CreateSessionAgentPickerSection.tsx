import { safeTestIdPart } from "../../lib/testId";
import type { AvailableAgents } from "./useAvailableAgents";
import { AgentKindBadge } from "./AgentKindBadge";
import type { AgentPlacementChoice } from "./agentPlacementChoice";

export interface CreateSessionAgentPickerSectionProps {
  availableAgents: AvailableAgents;
  selectedAgentIds: string[];
  toggleAgent: (agentId: string) => void;
  /** The picked agents whose conversations work on a clone of their own; the rest are `shared`. */
  clonePlacedAgentIds: string[];
  setAgentPlacement: (agentId: string, placement: AgentPlacementChoice) => void;
}

/**
 * The specialized-agent multi-select, shared by the cursor-cli and claude-cli managed-codebase
 * blocks. Every host's agents are listed together, so each option names the host that offers it
 * and submits the qualified id — picking "explorer" here cannot silently start another host's
 * agent of the same name. A host that could not be listed is one row; the rest stay on offer.
 *
 * A picked agent also carries its placement: `shared` (the session's worktree, the default) or
 * `clone` (a worktree of each conversation's own). The select is offered only once the agent is
 * picked, so an unpicked agent has no placement to choose.
 */
export function CreateSessionAgentPickerSection({
  availableAgents,
  selectedAgentIds,
  toggleAgent,
  clonePlacedAgentIds,
  setAgentPlacement,
}: CreateSessionAgentPickerSectionProps) {
  return (
    <div data-testid="create-session-agent-picker-section" className="space-y-1">
      {availableAgents.failures.map((failure) => (
        <p
          key={failure.daemonInstanceId}
          data-testid={`create-session-agent-host-error-${safeTestIdPart(failure.daemonInstanceId)}`}
          className="text-sm text-destructive"
        >
          {`${failure.daemonInstanceId}: ${failure.message}`}
        </p>
      ))}
      {availableAgents.agents.length === 0 && availableAgents.failures.length === 0 ? (
        <p className="text-sm text-muted-foreground">No specialized agents available</p>
      ) : (
        availableAgents.agents.map((agent) => (
          <label
            key={agent.agentId}
            className="flex items-center gap-2 text-sm text-muted-foreground"
          >
            <input
              data-testid={`create-session-agent-${safeTestIdPart(agent.agentId)}`}
              type="checkbox"
              className="h-4 w-4 rounded border-input"
              checked={selectedAgentIds.includes(agent.agentId)}
              onChange={() => toggleAgent(agent.agentId)}
            />
            <span>{agent.label || agent.name}</span>
            <span
              data-testid={`create-session-agent-${safeTestIdPart(agent.agentId)}-host`}
              className="text-xs"
            >
              {agent.daemonInstanceId}
            </span>
            <AgentKindBadge
              kind={agent.kind}
              harness={agent.harness}
              testId={`create-session-agent-${safeTestIdPart(agent.agentId)}-kind`}
            />
            {selectedAgentIds.includes(agent.agentId) && (
              <select
                data-testid={`create-session-agent-${safeTestIdPart(agent.agentId)}-placement`}
                aria-label={`Placement of ${agent.label || agent.name}`}
                className="rounded-md border border-input bg-background px-1 text-xs"
                value={clonePlacedAgentIds.includes(agent.agentId) ? "clone" : "shared"}
                onChange={(event) =>
                  setAgentPlacement(agent.agentId, event.target.value as AgentPlacementChoice)
                }
              >
                <option value="shared">shared</option>
                <option value="clone">clone</option>
              </select>
            )}
          </label>
        ))
      )}
    </div>
  );
}
