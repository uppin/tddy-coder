/**
 * The new-session form's **Agent** control: the agent a tool session is started *as*, listed across
 * every common-room host and labelled by the host offering it.
 *
 * Its own component because the fan-out gives the control three things the form's other selects do
 * not have — per-host error rows above it, option values qualified by host, and a pick that also
 * settles the session's host — and because the pane it came out of is long enough already.
 *
 * Presentational: the catalog, the selection and what a pick means are all the caller's. See
 * `useSelectableAgents` for where the rows come from and `selectableAgentOptions` for how they are
 * keyed and captioned.
 *
 * Feature: docs/ft/web/session-agent-catalog-fan-out.md
 */

import { safeTestIdPart } from "../../lib/testId";
import type { HostReadFailure } from "../../rpc/useHostFanOut";
import { inputClass, labelClass } from "./createSessionFormStyles";
import {
  selectableAgentText,
  selectableAgentValue,
  type SelectableAgent,
} from "./selectableAgentOptions";

interface CreateSessionAgentSelectProps {
  /** The agents this form may offer, in the order they are listed. */
  readonly agents: readonly SelectableAgent[];
  /** Hosts that could not be asked. Rendered above the select, one row each, never swallowed. */
  readonly failures: readonly HostReadFailure[];
  /** Whether a host can be named at all — off when no common room advertises one. */
  readonly hostsAdvertised: boolean;
  /** The value of the option to show as selected, or `""` when no agent is selectable. */
  readonly selectedValue: string;
  readonly onPick: (agent: SelectableAgent) => void;
}

export function CreateSessionAgentSelect({
  agents,
  failures,
  hostsAdvertised,
  selectedValue,
  onPick,
}: CreateSessionAgentSelectProps) {
  return (
    <div>
      <label className={labelClass} htmlFor="create-session-agent">
        Agent
      </label>
      {failures.map((failure) => (
        <p
          key={failure.daemonInstanceId}
          data-testid={`create-session-agent-select-host-error-${safeTestIdPart(failure.daemonInstanceId)}`}
          className="text-sm text-destructive"
        >
          {`${failure.daemonInstanceId}: ${failure.message}`}
        </p>
      ))}
      <select
        id="create-session-agent"
        data-testid="create-session-agent-select"
        className={inputClass}
        value={selectedValue}
        onChange={(e) => {
          // The picked row is looked up, never decoded: an agent id may itself contain an `@`, and
          // the row already carries the host the option was built from.
          const picked = agents.find(
            (a) => selectableAgentValue(a, hostsAdvertised) === e.target.value,
          );
          // A value with no row behind it can only come from a list that has since changed; keeping
          // the current pair beats guessing which agent and host it meant.
          if (!picked) return;
          onPick(picked);
        }}
      >
        {agents.length === 0 && failures.length === 0 ? (
          <option value="" disabled data-testid="create-session-agent-empty-option">
            No agents available
          </option>
        ) : (
          agents.map((a) => {
            const value = selectableAgentValue(a, hostsAdvertised);
            return (
              <option
                // Qualified whatever the value ends up being: the key identifies the (agent, host)
                // pair, and reusing the value's format keeps the two from drifting apart.
                key={selectableAgentValue(a, true)}
                value={value}
                data-testid={`create-session-agent-select-option-${safeTestIdPart(value)}`}
              >
                {selectableAgentText(a, hostsAdvertised)}
              </option>
            );
          })
        )}
      </select>
    </div>
  );
}
