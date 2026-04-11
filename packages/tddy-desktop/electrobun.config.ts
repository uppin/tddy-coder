import type { ElectrobunConfig } from "electrobun/bun";

const config: ElectrobunConfig = {
  app: {
    name: "Tddy Desktop",
    identifier: "dev.tddy.desktop",
    version: "0.1.0",
    description: "Native shell for tddy-web and Codex OAuth relay",
  },
  build: {
    bun: {
      entrypoint: "src/bun/index.ts",
    },
  },
};

export default config;
