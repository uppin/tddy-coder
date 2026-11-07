import type { MediaPort } from "tddy-client-web";
import { ParticipantVideoPreview } from "./ParticipantVideoPreview";
import { RemoteVideo } from "./RemoteVideo";
import { useHasCameraVideo } from "./useHasCameraVideo";
import { usePresenterRoom } from "./usePresenterRoom";
import { useScreenSharingInput } from "./useScreenSharingInput";

/** The LiveKit implementation of the media port: tracks, presenter room and screen-sharing input. */
export const livekitMedia: MediaPort = {
  usePresenterRoom,
  useScreenSharingInput,
  useHasCameraVideo,
  RemoteVideo,
  VideoPreview: ParticipantVideoPreview,
};
