import { describe, expect, test } from "bun:test";
import { existsSync, readdirSync, readFileSync } from "node:fs";
import { homedir } from "node:os";
import { join } from "node:path";

/**
 * `.github/actions/webrtc-prebuilt` caches the libwebrtc archive webrtc-sys-build downloads, under the
 * tag that crate hard-codes. A bump of the locked crate must revisit the pinned tag, or CI would keep
 * pointing LK_CUSTOM_WEBRTC at a library built for another webrtc-sys.
 */

const REPO_ROOT = join(import.meta.dir, "..");
const ACTION = readFileSync(join(REPO_ROOT, ".github", "actions", "webrtc-prebuilt", "action.yml"), "utf8");

function pinned(name: string): string {
  const match = new RegExp(`^#\\s+${name}: (\\S+)$`, "m").exec(ACTION);
  if (!match) throw new Error(`action.yml does not pin ${name}`);
  return match[1];
}

function lockedVersion(crate: string): string {
  const lock = readFileSync(join(REPO_ROOT, "Cargo.lock"), "utf8");
  const match = new RegExp(`\\[\\[package\\]\\]\\nname = "${crate}"\\nversion = "([^"]+)"`).exec(lock);
  if (!match) throw new Error(`Cargo.lock locks no ${crate}`);
  return match[1];
}

/** The crate's WEBRTC_TAG from the local cargo registry, when its source is unpacked there. */
function registryTag(version: string): string | null {
  const registry = join(process.env.CARGO_HOME ?? join(homedir(), ".cargo"), "registry", "src");
  if (!existsSync(registry)) return null;
  for (const index of readdirSync(registry)) {
    const lib = join(registry, index, `webrtc-sys-build-${version}`, "src", "lib.rs");
    if (existsSync(lib)) {
      return /pub const WEBRTC_TAG: &str = "([^"]+)";/.exec(readFileSync(lib, "utf8"))?.[1] ?? null;
    }
  }
  return null;
}

describe("prebuilt libwebrtc action", () => {
  test("pins the webrtc-sys-build version Cargo.lock locks", () => {
    expect(pinned("WEBRTC_SYS_BUILD_VERSION")).toBe(lockedVersion("webrtc-sys-build"));
  });

  test("downloads and caches under the pinned tag", () => {
    // Given
    const tag = pinned("WEBRTC_TAG");

    // Then
    expect(ACTION).toContain(`tag=${tag}`);
  });

  test("the pinned tag is the locked crate's WEBRTC_TAG (when its source is in the cargo registry)", () => {
    // Given
    const tag = registryTag(lockedVersion("webrtc-sys-build"));

    // Then — CI runners without the unpacked source rely on the version pin above
    if (tag !== null) expect(pinned("WEBRTC_TAG")).toBe(tag);
  });

  test("every workflow job that compiles the workspace uses the action", () => {
    // Given
    const ci = readFileSync(join(REPO_ROOT, ".github", "workflows", "ci.yml"), "utf8");
    const vm = readFileSync(join(REPO_ROOT, ".github", "workflows", "vm-tests.yml"), "utf8");

    // Then
    expect(ci.split("uses: ./.github/actions/webrtc-prebuilt").length - 1).toBeGreaterThanOrEqual(6);
    expect(vm).toContain("uses: ./.github/actions/webrtc-prebuilt");
  });
});
