# 2026-11-07 — LiveKit media behind `MediaPort` (#web-split 11/17, PR #855)

`src/media/` implements `MediaPort` (`livekitMedia`, registered by `livekitPorts().media`):
`usePresenterRoom` (was `tddy-web`'s `usePresenterLiveKitRoom`), `useScreenSharingInput`,
`screenSharingInput`, `presenterRoomTarget`, `participantCameraVideo` and
`ParticipantVideoPreviewDialog` moved here with their suites; `RemoteVideo`, `useHasCameraVideo` and
`ParticipantVideoPreview` are new. `ParticipantList` takes the `room` and asks the port per row — its
`participantHasCameraVideo` map, `VideoPreview` slot and local copy of the affordance rule are gone,
and the preview now gets the participant's camera track (it opened with none). `useCommonRoom` gained
`useCommonRoomHandle`. `ParticipantListVideoPreview.cy.tsx` moved here; roster specs mount through
`cypress/support/cameraRoster.tsx`. Participant-video and screen-input diagnostics log through
`tddyDebug` (`tddy:livekit:participant-video`, `tddy:livekit:screen-input`) instead of
`console.info`/`console.debug` under a `tddy-web` prefix.
