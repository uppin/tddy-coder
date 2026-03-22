import type { Meta, StoryObj } from "@storybook/react";
import { SessionRuntimeStatusBar } from "./SessionRuntimeStatusBar";

const meta: Meta<typeof SessionRuntimeStatusBar> = {
  component: SessionRuntimeStatusBar,
  title: "Components/SessionRuntimeStatusBar",
};

export default meta;

type Story = StoryObj<typeof SessionRuntimeStatusBar>;

export const Default: Story = {
  args: {
    statusLine:
      "GreenComplete | acceptance-tests | stub snapshot — session runtime status bar (Storybook)",
  },
};
