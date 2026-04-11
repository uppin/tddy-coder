import { describe, expect, it } from "bun:test";
import { Track } from "livekit-client";
import {
  hasRenderableCameraVideo,
  shouldShowParticipantVideoAffordance,
} from "./participantCameraVideo";

/**
 * Unit acceptance tests for camera/track visibility helpers (PRD Testing Plan).
 * Implementation lives in `./participantCameraVideo` — tests must fail until it exists.
 */
describe("participantCameraVideo (hasRenderableCameraVideo)", () => {
  it("returns false when the participant has no renderable camera video track", () => {
    const participant = {
      getTrackPublications: () => [],
      isLocal: false,
    };
    expect(hasRenderableCameraVideo(participant as never)).toBe(false);
  });

  it("returns true when a subscribed remote camera publication can be rendered", () => {
    const participant = {
      getTrackPublications: () => [
        {
          kind: Track.Kind.Video,
          source: Track.Source.Camera,
          isSubscribed: true,
          videoTrack: {},
        },
      ],
      isLocal: false,
    };
    expect(hasRenderableCameraVideo(participant as never)).toBe(true);
  });
});

describe("participantCameraVideo (shouldShowParticipantVideoAffordance)", () => {
  it("returns true when the injected map marks the identity as having camera video", () => {
    expect(shouldShowParticipantVideoAffordance({ "peer-a": true }, "peer-a")).toBe(true);
  });
});
