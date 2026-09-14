/**
 * SSH connections cell: the explicit OpenSSH `Host` aliases on this host.
 *
 * Three states, kept distinct because each is a different next action:
 *
 * | State | What an operator does |
 * |---|---|
 * | aliases listed | pick one later as a session destination |
 * | none | LocalShell is the only choice; this host has no Host aliases |
 * | could not check | do not treat this as "no destinations" |
 *
 * Honesty: a failed read is never rendered as an empty list.
 */

export interface HostRowSshConfigProps {
  instanceId: string;
}

export function HostRowSshConfig({ instanceId }: HostRowSshConfigProps) {
  // TODO(ssh-config): implement — fetch ListSshConfigHosts and render aliases / empty / failed.
  return (
    <span
      data-testid={`hosts-row-${instanceId}-ssh-config`}
      className="flex items-baseline gap-1 text-xs text-muted-foreground"
    >
      <span className="mr-1 opacity-70">ssh</span>
    </span>
  );
}
