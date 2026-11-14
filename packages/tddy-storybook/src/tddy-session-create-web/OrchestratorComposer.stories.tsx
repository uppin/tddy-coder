import type { Meta, StoryObj } from "@storybook/react";
import { OrchestratorComposer } from "tddy-session-create-web";

const meta: Meta<typeof OrchestratorComposer> = {
  title: "tddy-session-create-web/OrchestratorComposer",
  component: OrchestratorComposer,
  args: {
    projectId: "proj-beta",
    daemonInstanceId: "host-a",
    sessionToken: "story-token",
    onStarted: () => {},
  },
  decorators: [
    (Story) => (
      <div style={{ height: 600, display: "flex", flexDirection: "column" }}>
        <Story />
      </div>
    ),
  ],
};

export default meta;

type Story = StoryObj<typeof OrchestratorComposer>;

export const Empty: Story = {};
