/** Metadata builders for the room suites. For bun:test only. */

/** Returns a JSON string for a daemon advertisement in the LiveKit common room. */
export function aDaemonAdvertisementMeta(fields: {
  instanceId?: string;
  label?: string;
} = {}): string {
  return JSON.stringify({
    instance_id: fields.instanceId ?? "my-host",
    label: fields.label ?? "my-host (this daemon)",
  });
}

/** Returns a JSON string for a Codex OAuth metadata payload. */
export function aCodexOAuthMeta(fields: {
  pending?: boolean;
  authorizeUrl?: string;
  callbackPort?: number;
  state?: string;
} = {}): string {
  return JSON.stringify({
    codex_oauth: {
      pending: fields.pending ?? true,
      authorize_url: fields.authorizeUrl ?? "https://auth.example.com/oauth/authorize",
      callback_port: fields.callbackPort ?? 8765,
      state: fields.state ?? "test-state",
    },
  });
}

