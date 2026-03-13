import path from "path";
import { fileURLToPath } from "url";
import type { StorybookConfig } from "@storybook/react-vite";
import { mergeConfig } from "vite";

const projectRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");

const config: StorybookConfig = {
  framework: "@storybook/react-vite",
  stories: ["../src/**/*.stories.@(ts|tsx)"],
  addons: [],
  core: {
    allowedHosts: true,
  },
  async viteFinal(config) {
    const merged = mergeConfig(config, {
      root: projectRoot,
      optimizeDeps: {
        ...config.optimizeDeps,
        include: [
          ...(config.optimizeDeps?.include ?? []),
          "ghostty-web",
          "react",
          "react-dom",
        ],
      },
    });
    merged.server = {
      ...merged.server,
      host: "0.0.0.0",
      allowedHosts: true,
    };
    return merged;
  },
};

export default config;
