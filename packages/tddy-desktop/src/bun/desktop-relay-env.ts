import { existsSync, readFileSync } from "fs";
import { join } from "path";

import { DESKTOP_DEV_CONFIG_FILENAME } from "./embedded-daemon";

/** Minimal line scan of `dev.desktop.yaml` for OAuth relay (no full YAML parser). */
export function parseDevDesktopYamlForOAuthRelay(yaml: string): {
  rpcBase: string;
  livekitUrl?: string;
  commonRoom?: string;
} | null {
  const lines = yaml.split("\n");
  let i = 0;
  let webHost = "127.0.0.1";
  let webPort = 8899;
  let livekitUrl: string | undefined;
  let commonRoom: string | undefined;

  const scanIndentedBlock = (
    start: number,
    visit: (line: string) => void
  ): number => {
    let j = start;
    for (; j < lines.length; j++) {
      const line = lines[j];
      const t = line.trim();
      if (t === "" || t.startsWith("#")) {
        continue;
      }
      if (/^\S/.test(line)) {
        break;
      }
      visit(line);
    }
    return j;
  };

  while (i < lines.length) {
    const raw = lines[i].trim();
    if (raw === "" || raw.startsWith("#")) {
      i++;
      continue;
    }
    if (raw === "listen:") {
      i = scanIndentedBlock(i + 1, (line) => {
        const hm = line.match(/^\s+web_host:\s*"?([^"#\s]+)"?\s*$/);
        const pm = line.match(/^\s+web_port:\s*(\d+)\s*$/);
        if (hm) webHost = hm[1];
        if (pm) webPort = Number(pm[1]);
      });
      continue;
    }
    if (raw === "livekit:") {
      i = scanIndentedBlock(i + 1, (line) => {
        const um = line.match(/^\s+url:\s*(\S+)\s*$/);
        const rm = line.match(/^\s+common_room:\s*(\S+)\s*$/);
        if (um) livekitUrl = um[1];
        if (rm) commonRoom = rm[1];
      });
      continue;
    }
    i++;
  }

  if (!livekitUrl || !commonRoom) {
    return null;
  }
  return {
    rpcBase: `http://${webHost}:${webPort}/rpc`,
    livekitUrl,
    commonRoom,
  };
}

/** Read repo-root desktop dev YAML and derive Connect-RPC + LiveKit settings for embedded daemon / web. */
export function inferOAuthRelayEnvFromDevDesktop(repoRoot: string): {
  rpcBase: string;
  livekitUrl: string;
  commonRoom: string;
} | null {
  const path = join(repoRoot, DESKTOP_DEV_CONFIG_FILENAME);
  if (!existsSync(path)) {
    return null;
  }
  try {
    const text = readFileSync(path, "utf-8");
    return parseDevDesktopYamlForOAuthRelay(text);
  } catch {
    return null;
  }
}
