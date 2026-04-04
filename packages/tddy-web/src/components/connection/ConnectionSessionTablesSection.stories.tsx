import type { Meta, StoryObj } from "@storybook/react";
import { SessionTablesStoryLayout } from "./ConnectionSessionTablesSection.demo";

const meta: Meta<typeof SessionTablesStoryLayout> = {
  title: "connection/ConnectionSessionTablesSection",
  component: SessionTablesStoryLayout,
  parameters: {
    layout: "centered",
  },
};

export default meta;

type Story = StoryObj<typeof SessionTablesStoryLayout>;

export const WideHost: Story = {
  args: { outerWidthPx: 960 },
};

export const NarrowHost: Story = {
  args: { outerWidthPx: 360 },
};
