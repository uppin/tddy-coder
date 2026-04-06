/**
 * Presentation policy for multiple concurrent terminal attachments (Connection screen).
 */

/**
 * Whether connecting session B implicitly disconnects prior attachments.
 * PRD: always false (unbounded concurrent attachments).
 */
export function detachOthersWhenAddingSecondSession(): boolean {
  console.debug("[tddy][multiSessionPresentation] detachOthersWhenAddingSecondSession → false");
  return false;
}
