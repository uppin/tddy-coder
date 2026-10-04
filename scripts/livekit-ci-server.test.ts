import { afterEach, describe, expect, test } from "bun:test";
import { chmodSync, mkdtempSync, readFileSync, rmSync, writeFileSync, existsSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

const REPO_ROOT = join(import.meta.dir, "..");
const SERVER_SCRIPT = join(import.meta.dir, "livekit-ci-server.sh");
const NEXTEST_CONFIG = join(REPO_ROOT, ".config", "nextest.toml");

/**
 * A sandbox in which `docker` and `curl` are stubs that record what they were asked and answer as
 * a healthy (or dead) LiveKit server would, so the script is exercised without a Docker daemon.
 */
class ACiRunner {
  readonly dir = mkdtempSync(join(tmpdir(), "livekit-ci-server-"));
  readonly dockerLog = join(this.dir, "docker.log");
  readonly githubEnv = join(this.dir, "github_env");

  constructor(options: { apiAnswers: boolean }) {
    this.stub(
      "docker",
      `echo "$@" >> "${this.dockerLog}"
case "$1" in
  run) echo deadbeefcafe ;;
  rm) if [ -f "${this.dir}/removed" ]; then echo "Error: No such container" >&2; exit 1; fi; touch "${this.dir}/removed" ;;
esac`,
    );
    this.stub("curl", options.apiAnswers ? "exit 0" : "exit 22");
    writeFileSync(this.githubEnv, "");
  }

  private stub(name: string, body: string) {
    const path = join(this.dir, name);
    writeFileSync(path, `#!/usr/bin/env bash\n${body}\n`);
    chmodSync(path, 0o755);
  }

  run(command: "start" | "stop") {
    const result = Bun.spawnSync([SERVER_SCRIPT, command], {
      env: {
        ...process.env,
        PATH: `${this.dir}:${process.env.PATH}`,
        GITHUB_ENV: this.githubEnv,
        LIVEKIT_CI_READY_TIMEOUT_SECS: "2",
      },
    });
    return {
      status: result.exitCode,
      stderr: result.stderr.toString(),
      docker: existsSync(this.dockerLog) ? readFileSync(this.dockerLog, "utf8") : "",
      githubEnv: readFileSync(this.githubEnv, "utf8"),
    };
  }

  cleanup() {
    rmSync(this.dir, { recursive: true, force: true });
  }
}

let runner: ACiRunner | undefined;
afterEach(() => runner?.cleanup());

function aHealthyRunner() {
  runner = new ACiRunner({ apiAnswers: true });
  return runner;
}

describe("starting the server", () => {
  test("start_publishes_each_port_on_the_same_number_inside_and_outside_the_container", () => {
    const { docker, status } = aHealthyRunner().run("start");

    const published = [...docker.matchAll(/-p (\d+):(\d+)(\/udp)?/g)];
    expect(status).toBe(0);
    expect(published.length).toBe(3);
    for (const [, host, container] of published) {
      expect(host).toBe(container);
    }
  });

  test("start_uses_a_pinned_image_never_the_floating_master_tag", () => {
    const { docker } = aHealthyRunner().run("start");

    const image = docker.match(/livekit\/livekit-server(\S*)/);
    expect(image).not.toBeNull();
    expect(image![1]).not.toBe("");
    expect(image![1]).not.toBe(":master");
  });

  test("start_exports_the_websocket_url_for_the_rest_of_the_job", () => {
    const { githubEnv } = aHealthyRunner().run("start");

    expect(githubEnv).toMatch(/^LIVEKIT_TESTKIT_WS_URL=ws:\/\/127\.0\.0\.1:\d+$/m);
  });

  test("start_fails_loudly_when_the_api_never_answers", () => {
    runner = new ACiRunner({ apiAnswers: false });

    const { status, stderr } = runner.run("start");

    expect(status).not.toBe(0);
    expect(stderr).toMatch(/127\.0\.0\.1:\d+/);
  });
});

describe("stopping the server", () => {
  test("stop_removes_the_container_and_is_safe_to_run_twice", () => {
    const ci = aHealthyRunner();
    ci.run("start");

    const first = ci.run("stop");
    const second = ci.run("stop");

    expect(first.docker).toMatch(/rm -f/);
    expect(first.status).toBe(0);
    expect(second.status).toBe(0);
  });
});

describe("hang protection", () => {
  test("the_docker_override_kills_a_stuck_test", () => {
    const config = readFileSync(NEXTEST_CONFIG, "utf8");

    const overrides = config.split("[[profile.ci.overrides]]").slice(1);
    const dockerOverride = overrides.find((block) => /test-group\s*=\s*"docker"/.test(block));

    expect(dockerOverride).toBeDefined();
    expect(dockerOverride).toMatch(/slow-timeout\s*=\s*\{[^}]*terminate-after/);
  });
});
