/**
 * LiveKit participant camera track helpers (PRD: video affordance / preview).
 */

import { Track } from "livekit-client";

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
  console.debug("[tddy-web:participant-video] hasRenderableCameraVideo: evaluating participant", {
    isLocal: (participant as MinimalParticipant)?.isLocal,
  });

  if (participant === null || typeof participant !== "object") {
    console.info("[tddy-web:participant-video] hasRenderableCameraVideo: non-object participant");
    return false;
  }

  const p = participant as MinimalParticipant;
  const getPubs = p.getTrackPublications;
  if (typeof getPubs !== "function") {
    console.info("[tddy-web:participant-video] hasRenderableCameraVideo: missing getTrackPublications");
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
      console.debug("[tddy-web:participant-video] hasRenderableCameraVideo: camera pub without videoTrack", {
        source,
      });
      continue;
    }

    if (p.isLocal === true) {
      if (pub.isMuted === true) {
        console.debug("[tddy-web:participant-video] hasRenderableCameraVideo: local camera muted");
        continue;
      }
      console.info("[tddy-web:participant-video] hasRenderableCameraVideo: local camera renderable");
      return true;
    }

    if (pub.isSubscribed !== true) {
      console.debug("[tddy-web:participant-video] hasRenderableCameraVideo: remote camera not subscribed");
      continue;
    }

    console.info("[tddy-web:participant-video] hasRenderableCameraVideo: remote camera renderable");
    return true;
  }

  console.debug("[tddy-web:participant-video] hasRenderableCameraVideo: no renderable camera publication");
  return false;
}

/**
 * Whether to show the per-row video affordance for this identity.
 * When `participantHasCameraVideo` is provided (tests or interim wiring), entries must be
 * explicitly `true` to show the control. Omitted map hides affordances until live track state
 * is merged in ConnectionScreen.
 */
export function shouldShowParticipantVideoAffordance(
  participantHasCameraVideo: Record<string, boolean> | undefined,
  identity: string,
): boolean {
  console.debug("[tddy-web:participant-video] shouldShowParticipantVideoAffordance", {
    identity,
    hasMap: participantHasCameraVideo !== undefined,
    value: participantHasCameraVideo?.[identity],
  });
  if (!participantHasCameraVideo) return false;
  return participantHasCameraVideo[identity] === true;
}
