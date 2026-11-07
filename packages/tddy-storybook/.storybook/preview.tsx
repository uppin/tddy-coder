import type { Preview } from "@storybook/react-vite";
import {
  MediaPortProvider,
  PresencePortProvider,
  RoomRpcPortProvider,
} from "tddy-client-web";
import { aMediaDouble, aPresenceDouble, aRoomRpcDouble, withClients } from "tddy-client-web/testkit";
import { anInMemoryRpcBackend } from "tddy-connectrpc-testkit";
import "tddy-ui-web/theme.css";

/**
 * Every story mounts inside the port doubles: the clients, navigation and auth ports over one
 * in-memory RPC backend, and the presence, room-RPC and media ports. A story needing a
 * particular backend answer or participant list builds its own doubles and is not affected —
 * this is the default world a component finds itself in.
 */
const backend = anInMemoryRpcBackend();
const presence = aPresenceDouble();
const roomRpc = aRoomRpcDouble(backend);
const media = aMediaDouble();

const preview: Preview = {
  parameters: {
    backgrounds: { disable: true },
  },
  decorators: [
    (Story) =>
      withClients(
        <PresencePortProvider port={presence}>
          <RoomRpcPortProvider port={roomRpc}>
            <MediaPortProvider port={media}>
              <div className="dark min-h-svh bg-background text-foreground">
                <Story />
              </div>
            </MediaPortProvider>
          </RoomRpcPortProvider>
        </PresencePortProvider>,
        backend,
      ),
  ],
};

export default preview;
