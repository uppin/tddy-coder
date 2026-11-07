/**
 * The single definition of how a runtime string becomes part of a `data-testid`.
 *
 * Room names and participant identities (`livekit.common_room`, `daemon-pr-stack/0001`) carry
 * characters that do not belong in a test id, so components collapse them before interpolating.
 * Cypress's `cypress/support/testIds.ts` builds the *same* ids to select those elements, so the two
 * must agree exactly — a private copy on either side would make every dynamic selector silently miss
 * instead of failing loudly. Both import this.
 */

/** Collapse everything outside `[A-Za-z0-9_-]` to `_`, so the result is safe inside a test id. */
export function safeTestIdPart(part: string): string {
  return part.replace(/[^a-zA-Z0-9_-]/g, "_");
}
