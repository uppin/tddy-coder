/**
 * The question a host is waiting on, for whichever surface can answer it.
 *
 * Subscribes to `HostService.StreamHostPrompts` for one host and surfaces its current
 * outstanding prompt. Nothing else in this app is server-initiated except the ACP bidi stream, so
 * this is the whole of "the daemon asked us something" — the hook returns the prompt and the caller
 * decides what dialog it deserves.
 *
 * `null` for `hostId` subscribes to nothing, the same tri-state `useHostStats` uses: a row that
 * nothing routes to must not silently read another host's prompts.
 *
 * The reading itself is `subscribeHostPrompts`, which is where the teardown lives — unmounting
 * closes the stream even when the host has never raised a prompt, which is the normal case for this
 * feed and which a `for await` parked on its first frame could not do.
 *
 * Feature: `docs/ft/web/hosts-screen-add-key.md`
 */

import { useEffect, useState } from "react";
import { HostService } from "../gen/host_pb";
import { subscribeHostPrompts, type HostPromptEventLike } from "./hostPromptsSubscription";
import { useHostClient } from "./connections/registry";
import { useAuthContext } from "../hooks/authProvider";

export type { HostPromptEventLike };

/**
 * Subscribe to `hostId`'s prompt feed and return the prompt it is currently waiting on.
 *
 * @param hostId the host to read; `null` to subscribe to nothing.
 * @returns the outstanding prompt, or `null` when the host is not asking anything.
 */
export function useHostPrompts(hostId: string | null): HostPromptEventLike | null {
  const client = useHostClient(HostService, hostId);
  const { sessionToken } = useAuthContext();
  const [prompt, setPrompt] = useState<HostPromptEventLike | null>(null);

  useEffect(() => {
    if (!client || hostId === null) {
      // Nothing routes here, so there is nothing outstanding either. A prompt left standing from an
      // earlier feed would offer to answer a question no host is waiting on any more.
      setPrompt(null);
      return;
    }

    // A question belongs to the feed that raised it: a host that went away and came back is not
    // still waiting on what it asked before.
    setPrompt(null);

    const subscription = subscribeHostPrompts(
      (signal) =>
        client.streamHostPrompts(
          { sessionToken: sessionToken ?? "", daemonInstanceId: hostId },
          { signal },
        ),
      (event) => setPrompt(event),
      (error) => {
        // Not the caller's to render: this hook reports what a host is asking, and a feed that died
        // is asking nothing. Said out loud all the same, because a dead feed and an idle one look
        // identical from here.
        console.debug("[useHostPrompts] host prompt stream ended", hostId, error);
      },
    );

    return () => subscription.unsubscribe();
  }, [client, hostId, sessionToken]);

  return prompt;
}
