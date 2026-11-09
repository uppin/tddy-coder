# 2026-11-07 — Media through `MediaPort`; no `livekit-client` import left (#web-split 11/17, PR #855)

`ScreenSharingOverlay` and `VncOverlay` render the port's `RemoteVideo` and forward input through
`useMediaPort().useScreenSharingInput`; `SessionScreenSharingTab` / `SessionVncTab` take a
`RoomHandle`; `WorkflowChatScreen` and `PrStackScreen` join the presenter room through
`useMediaPort().usePresenterRoom`; `HostDesktopOverlay` uses `useCommonRoomHandle`; `LiveKitAppPage`
no longer lends a camera dialog. `usePresenterLiveKitRoom`, `useScreenSharingInput`,
`participantCameraVideo` and `ParticipantVideoPreviewDialog` left for `tddy-livekit-web`. The
input-forwarding, fullscreen, VNC and presenter-room specs stay here unchanged: they mount through
`AppClientsPorts`, which registers the LiveKit media port.
