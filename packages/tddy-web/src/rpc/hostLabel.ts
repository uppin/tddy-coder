/**
 * The one rule a host is named by in text — an error that says which host could not be reached, the
 * owning-host badge on a cross-host row, the host gate's title.
 *
 * Every daemon advertises itself from its own perspective, as `"<id> (this daemon)"`. Read by
 * anyone else that suffix is wrong — it names the daemon *speaking*, not the one the operator is
 * looking at — so it is dropped wherever a host is named. The selector, which lists hosts as they
 * advertise themselves, does not use this.
 */

import type { DaemonHost } from "../lib/participantRole";

/** The suffix a daemon appends to its own advertised label. */
export const SELF_ADVERTISED_LABEL_SUFFIX = " (this daemon)";

/** What a host is named from: the listed hosts, and the selection's remembered label. */
export interface HostLabelSource {
  readonly daemons: readonly DaemonHost[];
  readonly selectedInstanceId: string | null;
  /** The label the selected host was last listed under (see `useSelectedDaemon().selectedLabel`). */
  readonly selectedLabel: string | null;
}

/**
 * The display name of `instanceId`: its listed label — or, for the selected host while it is not
 * listed, the label it was last seen under — without the self-advertised suffix; its instance id
 * when it has never been listed. `""` names no host, which `hostNotConnected` says as "the selected
 * host".
 */
export function hostLabelOf(source: HostLabelSource, instanceId: string): string {
  if (!instanceId) return "";
  const listed = source.daemons.find((d) => d.instanceId === instanceId)?.label;
  const remembered = instanceId === source.selectedInstanceId ? source.selectedLabel : null;
  const label = listed || remembered || instanceId;
  return label.endsWith(SELF_ADVERTISED_LABEL_SUFFIX)
    ? label.slice(0, -SELF_ADVERTISED_LABEL_SUFFIX.length)
    : label;
}
