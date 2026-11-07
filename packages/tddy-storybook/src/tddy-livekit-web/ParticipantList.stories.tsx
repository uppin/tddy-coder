import type { Meta, StoryObj } from "@storybook/react";
import type { RoomParticipant } from "tddy-client-web";
import { ParticipantList } from "tddy-livekit-web";

const JOINED = 1_726_700_000_000;

function aParticipant(identity: string, role: RoomParticipant["role"]): RoomParticipant {
  return { identity, role, joinedAt: JOINED, metadata: "", codexOAuth: null };
}

// Every capability the list reads comes off the connection: a presence-bearing one has a roster.
const connection = { capabilities: new Set(["presence", "media"]) };

const meta: Meta<typeof ParticipantList> = {
  title: "tddy-livekit-web/ParticipantList",
  component: ParticipantList,
  args: { roomStatus: "connected", connectionError: null, connection },
};

export default meta;

type Story = StoryObj<typeof ParticipantList>;

export const Roster: Story = {
  args: {
    participants: [aParticipant("daemon-host-a", "daemon"), aParticipant("coder-1", "coder"), aParticipant("web-ada", "browser")],
  },
};

export const Connecting: Story = {
  args: { roomStatus: "connecting", participants: [] },
};

export const Errored: Story = {
  args: { roomStatus: "error", connectionError: "token rejected by the LiveKit server", participants: [] },
};
