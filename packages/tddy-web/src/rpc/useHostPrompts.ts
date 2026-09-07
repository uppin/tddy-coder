/**
 * The question a host is waiting on, for whichever surface can answer it.
 *
 * Subscribes to `ConnectionService.StreamHostPrompts` for one host and surfaces its current
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
 * PRD: `docs/ft/web/1-WIP/PRD-2026-09-06-agent-add-key.md`
 */

import type { HostPromptEventLike } from "./hostPromptsSubscription";

export type { HostPromptEventLike };

/**
 * Subscribe to `hostId`'s prompt feed and return the prompt it is currently waiting on.
 *
 * @param hostId the host to read; `null` to subscribe to nothing.
 * @returns the outstanding prompt, or `null` when the host is not asking anything.
 */
export function useHostPrompts(hostId: string | null): HostPromptEventLike | null {
  // TODO: unimplemented — the subscription, the token wiring and the current-prompt state are still
  // to be written.
  void hostId;
  throw new Error("useHostPrompts is not implemented");
}
