import type { Preview } from "@storybook/react";
import "../src/index.css";

const preview: Preview = {
  parameters: {
    backgrounds: { disable: true },
  },
  decorators: [
    (Story) => (
      <div className="dark min-h-svh bg-background text-foreground">
        <Story />
      </div>
    ),
  ],
};

export default preview;
