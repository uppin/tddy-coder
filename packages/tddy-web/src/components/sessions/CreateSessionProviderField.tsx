/**
 * Where a session's harness runs: its default (its own vendor), or one of the person's provider
 * accounts. The options and whether each is usable come from the daemon (`ListProviderRoutes`); the
 * web keeps no harness-to-provider matrix of its own.
 *
 * The field is offered only when there is a choice to make: a harness that cannot run on another
 * provider (Cursor), or one no account can drive, renders nothing.
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
  choices: ProviderRouteChoice[];
  /** `"builtin"` for the default (the vendor), else an account id. */
  value: string;
  onChange: (value: string) => void;
  /** Why the daemon could not list the accounts; shown in place of the select, never swallowed. */
  error?: string | null;
}

/** The select's value for the harness's own vendor. */
export const BUILT_IN_PROVIDER = "builtin";

export function CreateSessionProviderField({
  harnessRoutable,
  choices,
  value,
  onChange,
  error = null,
}: CreateSessionProviderFieldProps) {
  if (error !== null) {
    return (
      <div data-testid="create-session-provider-field">
        <span className={labelClass}>Provider</span>
        <p data-testid="create-session-provider-error" className="text-sm text-destructive">
          {error}
        </p>
      </div>
    );
  }
  if (!harnessRoutable || !choices.some((choice) => choice.supported)) {
    return null;
  }
  return (
    <div data-testid="create-session-provider-field">
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
        <option value={BUILT_IN_PROVIDER}>Default</option>
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
