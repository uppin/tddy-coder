import type { Meta, StoryObj } from "@storybook/react";
import { SessionConnectionOverlay } from "tddy-session-web";

const meta: Meta<typeof SessionConnectionOverlay> = {
  title: "tddy-session-web/SessionConnectionOverlay",
  component: SessionConnectionOverlay,
  // The overlay covers its parent's pane stack, so the parent is positioned.
  decorators: [
    (Story) => (
      <div style={{ position: "relative", height: 240, width: 420 }}>
        <Story />
      </div>
    ),
  ],
};

export default meta;

type Story = StoryObj<typeof SessionConnectionOverlay>;

export const Connecting: Story = {
  args: { status: "connecting" },
};

export const Failed: Story = {
  args: { status: "error" },
};

export const Connected: Story = {
  args: { status: "connected" },
};
