import { describe, expect, it } from "bun:test";
import { Track } from "livekit-client";
import {
  hasRenderableCameraVideo,
  shouldShowParticipantVideoAffordance,
} from "./participantCameraVideo";

/**
 * Unit acceptance tests for camera/track visibility helpers (PRD Testing Plan).
 * Implementation lives in `./participantCameraVideo` — tests must fail until it exists.
 *
 * Note: these tests build a minimal livekit Participant duck-type inline.
 * The `aFakeParticipant` builder in test-utils represents a RoomParticipant (domain
 * type), not the livekit Participant SDK type, so it does not apply here.
 */

describe("hasRenderableCameraVideo", () => {
  it("returns false when the participant has no track publications", () => {
    // Given
    const participant = { getTrackPublications: () => [], isLocal: false };

    // When / Then
    expect(hasRenderableCameraVideo(participant as never)).toBe(false);
  });

  it("returns true when a subscribed remote camera publication can be rendered", () => {
    // Given
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

    // When / Then
    expect(hasRenderableCameraVideo(participant as never)).toBe(true);
  });
});

describe("shouldShowParticipantVideoAffordance", () => {
  it("returns true when the injected map marks the identity as having camera video", () => {
    // Given
    const cameraMap = { "peer-a": true };

    // When / Then
    expect(shouldShowParticipantVideoAffordance(cameraMap, "peer-a")).toBe(true);
  });
});
