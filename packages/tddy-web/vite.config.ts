import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

export default defineConfig({
  plugins: [
    react({ babel: { plugins: ["babel-plugin-react-compiler"] } }),
  ],
  server: {
    proxy: {
      "/rpc": {
        target: `http://127.0.0.1:${process.env.DAEMON_PORT ?? 8899}`,
        changeOrigin: true,
      },
      "/api": {
        target: `http://127.0.0.1:${process.env.DAEMON_PORT ?? 8899}`,
        changeOrigin: true,
      },
    },
  },
});
