import { LiveKitRoomsContent } from "tddy-livekit-web";
import { AppShell } from "../shell/AppShell";

/**
 * The LiveKit presence screen (`#/livekit`): the app shell around `tddy-livekit-web`'s content — the
 * participants in the shared common room and every room on the LiveKit server.
 */
export function LiveKitAppPage({ onNavigate }: { onNavigate: (path: string) => void }) {
  return (
    <AppShell title="LiveKit" onNavigate={onNavigate} variant="scroll">
      <LiveKitRoomsContent />
    </AppShell>
  );
}
