/**
 * The text a participant's metadata card shows on the rooms panel.
 *
 * Kept beside the panel's other pure readouts (`liveKitRoomsState`) rather than inside the
 * component, so each of its branches is a unit test instead of a hover-and-read acceptance case.
 *
 * PRD: `docs/ft/web/livekit-rooms-panel.md`
 * Changeset: `livekit-rooms-panel`
 */

/**
 * A participant's metadata pretty-printed when it parses as JSON, the string verbatim when it does
 * not (the daemon relays what was published and the web does not validate it), and an explicit note
 * when nothing was published at all.
 */
export function metadataCardText(metadata: string): string {
  const trimmed = metadata.trim();
  if (!trimmed) return "No metadata published.";
  try {
    return JSON.stringify(JSON.parse(trimmed), null, 2);
  } catch {
    return metadata;
  }
}
