/**
 * LiveKit participant camera track helper (PRD: video affordance / preview).
 *
 * It does not ask whether the connection can carry video: that is one question, asked once, by
 * whoever renders the affordance (`ParticipantList`, through `useHasCapability`). Threading a
 * capability into a pure "does this publication have a camera track" predicate would give the
 * gating two homes, which is exactly the drift the single predicate exists to prevent — so on a
 * wire with no tracks this is simply not consulted.
 */

import { Track } from "livekit-client";
import { tddyDebug } from "tddy-client-web";

const dbg = tddyDebug("tddy:livekit:participant-video");

type MinimalPublication = {
  kind?: unknown;
  source?: unknown;
  isSubscribed?: boolean;
  isMuted?: boolean;
  videoTrack?: unknown;
};

type MinimalParticipant = {
  isLocal?: boolean;
  getTrackPublications?: () => Iterable<unknown>;
};

/**
 * Returns whether the participant currently has at least one camera video publication
 * the client can render (local enabled camera or subscribed remote camera).
 *
 * Uses the same rules as the LiveKit web SDK: camera {@link Track.Source.Camera}, video
 * {@link Track.Kind.Video}, an attached {@link TrackPublication#videoTrack}, and for remote
 * participants a subscribed publication.
 */
export function hasRenderableCameraVideo(participant: unknown): boolean {
  dbg("hasRenderableCameraVideo: evaluating participant", {
    isLocal: (participant as MinimalParticipant)?.isLocal,
  });

  if (participant === null || typeof participant !== "object") {
    dbg("hasRenderableCameraVideo: non-object participant");
    return false;
  }

  const p = participant as MinimalParticipant;
  const getPubs = p.getTrackPublications;
  if (typeof getPubs !== "function") {
    dbg("hasRenderableCameraVideo: missing getTrackPublications");
    return false;
  }

  for (const raw of getPubs()) {
    if (!raw || typeof raw !== "object") continue;
    const pub = raw as MinimalPublication;

    const kind = pub.kind;
    if (kind !== Track.Kind.Video && kind !== "video") continue;

    const source = pub.source;
    if (source !== Track.Source.Camera) continue;

    if (pub.videoTrack == null) {
      dbg("hasRenderableCameraVideo: camera pub without videoTrack", {
        source,
      });
      continue;
    }

    if (p.isLocal === true) {
      if (pub.isMuted === true) {
        dbg("hasRenderableCameraVideo: local camera muted");
        continue;
      }
      dbg("hasRenderableCameraVideo: local camera renderable");
      return true;
    }

    if (pub.isSubscribed !== true) {
      dbg("hasRenderableCameraVideo: remote camera not subscribed");
      continue;
    }

    dbg("hasRenderableCameraVideo: remote camera renderable");
    return true;
  }

  dbg("hasRenderableCameraVideo: no renderable camera publication");
  return false;
}
