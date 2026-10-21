/**
 * Where a session's harness runs: its own vendor ("Anthropic (built in)" for Claude Code), or one of
 * the person's provider accounts. The options and whether each is usable come from the daemon
 * (`ListProviderRoutes`); the web keeps no harness-to-provider matrix of its own.
 *
 * A harness that cannot run on any provider (Cursor) shows the daemon's reason instead of a select.
 */

import { inputClass, labelClass } from "./createSessionFormStyles";

/** One account as the field offers it. */
export interface ProviderRouteChoice {
  accountId: string;
  label: string;
  preset: string;
  supported: boolean;
  /** Why the harness cannot run on this account; empty when supported. */
  reason: string;
}

export interface CreateSessionProviderFieldProps {
  /** False when the harness cannot run on any provider account. */
  harnessRoutable: boolean;
  harnessReason: string;
  choices: ProviderRouteChoice[];
  /** `"builtin"` for the vendor, else an account id. */
  value: string;
  onChange: (value: string) => void;
}

/** The select's value for the harness's own vendor. */
export const BUILT_IN_PROVIDER = "builtin";

export function CreateSessionProviderField({
  harnessRoutable,
  harnessReason,
  choices,
  value,
  onChange,
}: CreateSessionProviderFieldProps) {
  if (!harnessRoutable) {
    return (
      <div>
        <span className={labelClass}>Provider</span>
        <p data-testid="create-session-provider-unavailable" className="text-sm text-muted-foreground">
          {harnessReason}
        </p>
      </div>
    );
  }
  return (
    <div>
      <label className={labelClass} htmlFor="create-session-provider">
        Provider
      </label>
      <select
        id="create-session-provider"
        data-testid="create-session-provider-select"
        className={inputClass}
        value={value}
        onChange={(e) => onChange(e.target.value)}
      >
        <option value={BUILT_IN_PROVIDER}>Anthropic (built in)</option>
        {choices.map((choice) => (
          <option
            key={choice.accountId}
            value={choice.accountId}
            disabled={!choice.supported}
            title={choice.supported ? undefined : choice.reason}
          >
            {choice.supported ? choice.label : `${choice.label} — ${choice.reason}`}
          </option>
        ))}
      </select>
    </div>
  );
}
