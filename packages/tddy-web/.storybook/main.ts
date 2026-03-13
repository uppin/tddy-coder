import type { StorybookConfig } from "@storybook/react-vite";
import { mergeConfig } from "vite";
import react from "@vitejs/plugin-react";

const config: StorybookConfig = {
  framework: "@storybook/react-vite",
  stories: ["../src/**/*.stories.@(ts|tsx)"],
  addons: [],
  async viteFinal(config) {
    return mergeConfig(config, {
      plugins: [
        react({ babel: { plugins: ["babel-plugin-react-compiler"] } }),
      ],
    });
  },
};

export default config;
