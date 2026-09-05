/**
 * Translation between a directory {@link HostDescriptor} and the `DaemonHost` the screens read.
 *
 * The two describe the same machine. `DaemonHost` is the common room's vocabulary — it came out of
 * a participant advertisement and names the field `instanceId` — and it is what `DaemonSelector`,
 * `resolveSelectedDaemonInstanceId` and the Start-Session form already speak. `HostDescriptor` is
 * the directory's, and additionally records *which source* said so.
 *
 * Keeping both, with one conversion in one place, is deliberate: renaming `DaemonHost` out of the
 * screens is a change to every host-facing surface in the app and belongs to no node of this stack.
 */

import type { DaemonHost } from "../../lib/participantRole";
import type { HostDescriptor } from "./types";

/**
 * Describe `host` as `sourceId`'s contribution to the directory.
 *
 * The optional fields are only set when the source actually advertised them: `undefined` and
 * "absent" are the same thing to every consumer, but an explicitly-present `undefined` would make
 * a descriptor unequal to one built from the same advertisement a render earlier.
 */
export function hostDescriptorOf(host: DaemonHost, sourceId: string): HostDescriptor {
  return {
    hostId: host.instanceId,
    label: host.label,
    sourceId,
    ...(host.reposBasePath !== undefined ? { reposBasePath: host.reposBasePath } : {}),
    ...(host.maxAttachmentBytes !== undefined
      ? { maxAttachmentBytes: host.maxAttachmentBytes }
      : {}),
  };
}

/** The same host in the vocabulary the daemon-mode screens read. */
export function daemonHostOf(descriptor: HostDescriptor): DaemonHost {
  return {
    instanceId: descriptor.hostId,
    label: descriptor.label,
    ...(descriptor.reposBasePath !== undefined ? { reposBasePath: descriptor.reposBasePath } : {}),
    ...(descriptor.maxAttachmentBytes !== undefined
      ? { maxAttachmentBytes: descriptor.maxAttachmentBytes }
      : {}),
  };
}
