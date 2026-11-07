import path from "path";
import { fileURLToPath } from "url";
import tailwindcss from "@tailwindcss/vite";
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
      plugins: [tailwindcss()],
      resolve: {
        dedupe: ["react", "react-dom"],
      },
      // Pre-bundled up front so the dev server never re-optimises (and reloads the page) mid-run.
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
      fs: { allow: [path.resolve(projectRoot, "../..")] },
    };
    return merged;
  },
};

export default config;
