import type { Meta, StoryObj } from "@storybook/react";
import { HostsScreen, type HostRow } from "tddy-hosts-web";

const NOW = 1_726_700_000_000;

function aHost(instanceId: string, label: string, overrides: Partial<HostRow> = {}): HostRow {
  return {
    instanceId,
    label,
    online: true,
    lastSeenUnixMs: BigInt(NOW),
    reposBasePath: "/home/dev/repos",
    isLocal: false,
    ...overrides,
  };
}

const meta: Meta<typeof HostsScreen> = {
  title: "tddy-hosts-web/HostsScreen",
  component: HostsScreen,
  args: { nowUnixMs: NOW },
};

export default meta;

type Story = StoryObj<typeof HostsScreen>;

export const Listed: Story = {
  args: {
    rows: [
      aHost("host-b", "build-box", { online: false, lastSeenUnixMs: BigInt(NOW - 3_600_000) }),
      aHost("host-a", "laptop", { isLocal: true }),
      aHost("host-c", "ci-runner"),
    ],
  },
};

export const Empty: Story = {
  args: { rows: [] },
};
