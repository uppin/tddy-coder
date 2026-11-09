import path from "path";
import { fileURLToPath } from "url";
import tailwindcss from "@tailwindcss/vite";
import type { StorybookConfig } from "@storybook/react-vite";
import { mergeConfig } from "vite";
import { prebundledDeps } from "../../../scripts/vite-prebundle";

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
      // Pre-bundled up front so the dev server never re-optimises (and reloads the page) mid-run —
      // the same list every package's Vite config uses (scripts/vite-prebundle.ts). Storybook keeps its
      // own entries (the stories); only the include list is shared.
      optimizeDeps: {
        ...config.optimizeDeps,
        include: [...(config.optimizeDeps?.include ?? []), ...prebundledDeps(projectRoot).include],
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
