import { safeTestIdPart } from "../../lib/testId";
import type { AvailableAgents } from "./useAvailableAgents";

export interface CreateSessionAgentPickerSectionProps {
  availableAgents: AvailableAgents;
  selectedAgentIds: string[];
  toggleAgent: (agentId: string) => void;
}

/**
 * The specialized-agent multi-select, shared by the cursor-cli and claude-cli managed-codebase
 * blocks. Every host's agents are listed together, so each option names the host that offers it
 * and submits the qualified id — picking "explorer" here cannot silently start another host's
 * agent of the same name. A host that could not be listed is one row; the rest stay on offer.
 */
export function CreateSessionAgentPickerSection({
  availableAgents,
  selectedAgentIds,
  toggleAgent,
}: CreateSessionAgentPickerSectionProps) {
  return (
    <div data-testid="create-session-managed-codebase-section" className="space-y-1">
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
          </label>
        ))
      )}
    </div>
  );
}
