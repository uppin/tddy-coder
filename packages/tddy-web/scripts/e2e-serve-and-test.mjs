#!/usr/bin/env node
import { spawn } from "child_process";
import getPort from "get-port";
import { fileURLToPath } from "url";
import { dirname, join } from "path";

const __dirname = dirname(fileURLToPath(import.meta.url));
const projectRoot = join(__dirname, "..");

async function main() {
  const port = await getPort();
  const baseUrl = `http://localhost:${port}`;

  const serve = spawn(
    "npx",
    ["http-server", "storybook-static", "-p", String(port), "-c-1", "--cors"],
    {
      cwd: projectRoot,
      stdio: ["ignore", "pipe", "pipe"],
    }
  );

  await new Promise((resolve, reject) => {
    const timeout = setTimeout(() => reject(new Error("Server start timeout")), 30000);
    const check = async () => {
      try {
        const res = await fetch(`${baseUrl}/iframe.html`);
        if (res.ok) {
          clearTimeout(timeout);
          resolve();
        }
      } catch {
        setTimeout(check, 200);
      }
    };
    setTimeout(check, 500);
  });

  const cypress = spawn(
    "cypress",
    ["run", "--e2e"],
    {
      cwd: projectRoot,
      stdio: "inherit",
      env: {
        ...process.env,
        CYPRESS_BASE_URL: baseUrl,
        ELECTRON_EXTRA_LAUNCH_ARGS: "--disable-gpu --no-sandbox",
      },
    }
  );

  const exitCode = await new Promise((resolve) => {
    cypress.on("close", resolve);
  });

  serve.kill();
  process.exit(exitCode ?? 1);
}

main().catch((err) => {
  console.error(err);
  process.exit(1);
});
