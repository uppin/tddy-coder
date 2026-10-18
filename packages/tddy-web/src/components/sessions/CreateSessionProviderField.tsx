/**
 * Where a session's harness runs: its own vendor ("Anthropic (built in)" for Claude Code), or one of
 * the person's provider accounts. The options and whether each is usable come from the daemon
 * (`ListProviderRoutes`); the web keeps no harness-to-provider matrix of its own.
 *
 * A harness that cannot run on any provider (Cursor) shows the daemon's reason instead of a select.
 */

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

export function CreateSessionProviderField(props: CreateSessionProviderFieldProps) {
  void props;
  // TODO(claude-route): implement
  return null;
}
