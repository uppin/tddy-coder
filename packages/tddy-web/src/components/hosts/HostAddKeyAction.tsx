/**
 * Load a private key into a host's ssh-agent, from the browser.
 *
 * This is the surface that *starts* the flow the rest of this node builds. `AddHostKey` blocks for
 * as long as the add takes: it raises a passphrase prompt on `StreamHostPrompts`, waits for the
 * ciphertext to come back on `AnswerHostPrompt`, unlocks the key and hands it to the agent. So this
 * component both issues that call and, through `useHostPrompts`, renders the dialog the same call
 * is waiting on.
 *
 * It is offered **only where there is an agent to add to**. A host whose agent did not answer needs
 * an agent started, not a key loaded, and offering an add there would put a control in front of an
 * operator that cannot do anything. `HostRowSshAgent` already draws that distinction for the
 * summary text; this reads the same block rather than restating the rule.
 *
 * The outcome is reported from `AddHostKeyOutcome`, not from `added` alone. A wrong passphrase is
 * worth retyping, an absent agent is not, and an expired prompt means answering faster — three
 * different next actions, which is the entire reason the response carries an enum.
 *
 * PRD: `docs/ft/web/1-WIP/PRD-2026-09-06-agent-add-key.md`
 */

import React from "react";
import type { HostSshAgent } from "../../gen/connection_pb";

export interface HostAddKeyActionProps {
  instanceId: string;
  /** This host's ssh-agent probe result — the same block `HostRowSshAgent` summarises. */
  sshAgent: HostSshAgent | undefined;
}

export function HostAddKeyAction({
  instanceId,
  sshAgent,
}: HostAddKeyActionProps): React.ReactElement | null {
  // TODO: unimplemented — the key field, the AddHostKey call, the prompt dialog and the outcome
  // reporting are still to be written.
  void instanceId;
  void sshAgent;
  throw new Error("HostAddKeyAction is not implemented");
}
