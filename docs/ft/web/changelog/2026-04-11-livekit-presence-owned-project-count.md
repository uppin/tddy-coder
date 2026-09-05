# 2026-04-11 — LiveKit presence: owned project count

- **tddy-web**: **`ParticipantList`** **Projects** column for **`owned_project_count`** in participant metadata (**`parseOwnedProjectCount`**, **`OWNED_PROJECT_COUNT_METADATA_KEY`**); em dash when the field is absent; **`useRoomParticipants`** supplies **`ownedProjectCount`** from LiveKit metadata; Cypress **`ParticipantList.cy.tsx`** covers render and metadata updates. Feature doc: [livekit-participant-owned-projects.md](../livekit-participant-owned-projects.md).
