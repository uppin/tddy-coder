import type { ComponentProps } from "react";
import { MediaPortProvider, type MediaPort } from "tddy-client-web";
import { aMediaDouble, aPresenceDouble } from "tddy-client-web/testkit";
import { ParticipantList } from "../../src";

const COMMON_ROOM = aPresenceDouble().room("common");

/**
 * Mount a roster whose participants named in `withCamera` have camera video in its room, as the
 * media port reports it. `media` overrides what the port renders (e.g. the preview dialog).
 */
export function mountRosterWithCameras(
  props: Omit<ComponentProps<typeof ParticipantList>, "room">,
  withCamera: readonly string[],
  media: Partial<MediaPort> = {},
) {
  const port = aMediaDouble(media);
  for (const identity of withCamera) port.setCameraVideo(COMMON_ROOM, identity, true);
  cy.mount(
    <MediaPortProvider port={port}>
      <ParticipantList {...props} room={COMMON_ROOM} />
    </MediaPortProvider>,
  );
}
