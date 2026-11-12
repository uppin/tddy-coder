import { inputClass, labelClass } from "./createSessionFormStyles";

export interface CreateSessionEffortFieldProps {
  /**
   * The effort levels the selected agent's spec declares (`AgentInfo.effort_levels`, served by the
   * target daemon), in its order. Empty for an agent that declares none: the field is not shown.
   */
  levels: readonly string[];
  /** The chosen level; empty for the agent's own default. */
  effort: string;
  setEffort: (effort: string) => void;
}

/**
 * Effort selector beside Model (`#orchestrators` 3/11): the reasoning effort a session is started
 * at, sent as `StartSessionRequest.effort`. Lists only what the daemon served for the agent — the
 * bundle names no agent's levels — with an empty choice first for the agent's own default, and
 * renders nothing for an agent without levels. Test id `create-session-effort-select`.
 *
 * PRD: docs/ft/daemon/1-WIP/PRD-2026-11-10-start-effort.md.
 */
export function CreateSessionEffortField({
  levels,
  effort,
  setEffort,
}: CreateSessionEffortFieldProps) {
  if (levels.length === 0) {
    return null;
  }
  return (
    <div>
      <label className={labelClass} htmlFor="create-session-effort">
        Effort
      </label>
      <select
        id="create-session-effort"
        data-testid="create-session-effort-select"
        className={inputClass}
        value={effort}
        onChange={(e) => setEffort(e.target.value)}
      >
        <option value="">Default</option>
        {levels.map((level) => (
          <option key={level} value={level}>
            {level}
          </option>
        ))}
      </select>
    </div>
  );
}
