import type { Meta, StoryObj } from "@storybook/react";
import { Button } from "tddy-ui-web";

const meta: Meta<typeof Button> = {
  title: "components/Button",
  component: Button,
};

export default meta;

type Story = StoryObj<typeof Button>;

export const Primary: Story = {
  render: () => <Button type="button">Primary</Button>,
};
