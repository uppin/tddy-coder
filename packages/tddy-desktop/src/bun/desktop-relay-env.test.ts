import { describe, expect, test } from "bun:test";
import {
  inferOAuthRelayEnvFromDevDesktop,
  parseDevDesktopYamlForOAuthRelay,
} from "./desktop-relay-env";

describe("parseDevDesktopYamlForOAuthRelay", () => {
  test("parses listen + livekit from dev.desktop-shaped YAML", () => {
    const yaml = `
listen:
  web_port: 8899
  web_host: 127.0.0.1

livekit:
  url: ws://192.168.1.10:7880
  api_key: devkey
  common_room: tddy-lobby
`;
    const r = parseDevDesktopYamlForOAuthRelay(yaml);
    expect(r).not.toBeNull();
    expect(r!.rpcBase).toBe("http://127.0.0.1:8899/rpc");
    expect(r!.livekitUrl).toBe("ws://192.168.1.10:7880");
    expect(r!.commonRoom).toBe("tddy-lobby");
  });

  test("returns null without livekit url or room", () => {
    expect(
      parseDevDesktopYamlForOAuthRelay("listen:\n  web_port: 1\n")
    ).toBeNull();
  });
});

describe("inferOAuthRelayEnvFromDevDesktop", () => {
  test("returns null for missing file", () => {
    expect(inferOAuthRelayEnvFromDevDesktop("/nonexistent/dir")).toBeNull();
  });
});
