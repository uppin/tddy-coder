import type { Meta, StoryObj } from "@storybook/react";
import { PlannedPrPanel } from "tddy-prstack-web";

const meta: Meta<typeof PlannedPrPanel> = {
  title: "tddy-prstack-web/PlannedPrPanel",
  component: PlannedPrPanel,
  args: { state: "open", isMobile: false, onClose: () => {} },
  // The panel is positioned against its parent, which must be `relative`.
  decorators: [
    (Story) => (
      <div style={{ position: "relative", height: 420, width: 720 }}>
        <Story />
      </div>
    ),
  ],
  render: (args) => (
    <PlannedPrPanel {...args}>
      <p style={{ padding: 12 }}>Planned PRs list goes here.</p>
    </PlannedPrPanel>
  ),
};

export default meta;

type Story = StoryObj<typeof PlannedPrPanel>;

export const Open: Story = {};

export const Mobile: Story = {
  args: { isMobile: true },
};

export const Closed: Story = {
  args: { state: "closed" },
};
