import type { Meta, StoryObj } from "@storybook/react";
import { RpcPlaygroundScreen, type InvokeResult } from "tddy-rpc-playground-web";

const services = [
  {
    name: "tddy.v1.EchoService",
    methods: [
      { name: "Echo", kind: "unary" as const },
      { name: "EchoStream", kind: "server_streaming" as const },
    ],
  },
  { name: "tddy.v1.TerminalService", methods: [{ name: "Stream", kind: "bidi_streaming" as const }] },
];

const meta: Meta<typeof RpcPlaygroundScreen> = {
  title: "tddy-rpc-playground-web/RpcPlaygroundScreen",
  component: RpcPlaygroundScreen,
  args: {
    services,
    presenceAvailable: true,
    participants: [
      { id: "coder-1", label: "coder-1" },
      { id: "coder-2", label: "coder-2" },
    ],
    selectedParticipant: "coder-1",
    onSelectParticipant: () => {},
    onInvoke: async (): Promise<InvokeResult> => ({ kind: "success", json: '{"message":"hello"}' }),
    onNavigate: () => {},
  },
};

export default meta;

type Story = StoryObj<typeof RpcPlaygroundScreen>;

export const Services: Story = {};

export const NoPresence: Story = {
  args: { presenceAvailable: false, participants: undefined },
};
