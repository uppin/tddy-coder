# 2026-11-07 — `MediaPort` final shape and its double (#web-split 11/17, PR #855)

`MediaPort` = `usePresenterRoom`, `useScreenSharingInput` (`inputUnavailable`), `useHasCameraVideo`,
`RemoteVideo` (`RemoteVideoProps`: `testId`, `videoRef`) and the new `VideoPreview`
(`ParticipantVideoPreviewProps`). `aMediaDouble` replaces the interim stub: `setCameraVideo`,
`presentRoom`, and an input recorder that listens on the real picture (`ForwardedInput`).
`PresencePort` and `RoomRpcPort` (including `useParticipantProbe`) are unchanged.
