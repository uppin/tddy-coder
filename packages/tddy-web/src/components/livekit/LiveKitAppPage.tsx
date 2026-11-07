import { LiveKitRoomsContent } from "tddy-livekit-web";
import { AppShell } from "../shell/AppShell";
import { ParticipantVideoPreviewDialog } from "../ParticipantVideoPreviewDialog";

/**
 * The LiveKit presence screen (`#/livekit`): the app shell around `tddy-livekit-web`'s content — the
 * participants in the shared common room and every room on the LiveKit server.
 *
 * The camera preview the roster opens is a media surface, so the app lends it
 * (TODO(media): `feature/web-split/media` moves it behind `MediaPort`).
 */
export function LiveKitAppPage({ onNavigate }: { onNavigate: (path: string) => void }) {
  return (
    <AppShell title="LiveKit" onNavigate={onNavigate} variant="scroll">
      <LiveKitRoomsContent VideoPreview={ParticipantVideoPreviewDialog} />
    </AppShell>
  );
}
