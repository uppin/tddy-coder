/** Metadata builders shared by the participant-role suites. For bun:test only. */

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

