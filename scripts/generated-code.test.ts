import { afterEach, describe, expect, test } from "bun:test";
import { mkdtempSync, mkdirSync, readFileSync, rmSync, writeFileSync, unlinkSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

const REPO_ROOT = join(import.meta.dir, "..");
const GATE = join(import.meta.dir, "generated-code.sh");

const GREETING_PROTO = `syntax = "proto3";
package greeting;

message Hello {
  string name = 1;
}
`;

/**
 * A miniature repository laid out the way the manifest expects: one proto root, one
 * package with a buf.gen.yaml, and a gen directory produced by the gate's own
 * `write` mode — so "fresh" means what the gate means by it, not what a fixture
 * file happens to say.
 */
class AGeneratedCodeRepo {
  readonly root = mkdtempSync(join(tmpdir(), "generated-code-"));
  private readonly manifest = join(this.root, "manifest");
  readonly genDir = join(this.root, "pkg", "gen");

  constructor() {
    mkdirSync(join(this.root, "proto"), { recursive: true });
    mkdirSync(join(this.root, "pkg"), { recursive: true });
    writeFileSync(join(this.root, "proto", "greeting.proto"), GREETING_PROTO);
    writeFileSync(
      join(this.root, "pkg", "buf.gen.yaml"),
      "version: v2\nplugins:\n  - local: protoc-gen-es\n    out: gen\n    opt: target=ts\n",
    );
    writeFileSync(this.manifest, "pkg | gen | ../proto\n");
    this.run("write");
  }

  withHandEditedGeneratedFile(): this {
    const file = join(this.genDir, "greeting_pb.ts");
    writeFileSync(file, `${readFileSync(file, "utf8")}// hand-edited\n`);
    return this;
  }

  withGeneratedFileDeleted(): this {
    unlinkSync(join(this.genDir, "greeting_pb.ts"));
    return this;
  }

  withCommittedFileNoProtoProduces(): this {
    writeFileSync(join(this.genDir, "retired_pb.ts"), "export const retired = 1;\n");
    return this;
  }

  check(): { exitCode: number; report: string } {
    const result = this.run("check");
    return { exitCode: result.exitCode, report: result.stdout.toString() };
  }

  private run(mode: "check" | "write") {
    return Bun.spawnSync({
      cmd: ["bash", GATE, mode, "--manifest", this.manifest, "--root", this.root],
      cwd: REPO_ROOT,
    });
  }

  discard(): void {
    rmSync(this.root, { recursive: true, force: true });
  }
}

let repo: AGeneratedCodeRepo | undefined;

const aGeneratedCodeRepo = () => {
  repo = new AGeneratedCodeRepo();
  return repo;
};

afterEach(() => {
  repo?.discard();
  repo = undefined;
});

/// Each test shells out to `buf generate` at least twice — once to build its "fresh" state and once
/// for the gate itself. Bun's 5s default is under that on a cold or loaded machine, and a timeout
/// there reads as a gate defect rather than as the clock it is.
const A_GENERATION_ROUND_TRIP = 60_000;

describe("the generated-code drift gate", () => {
  test("accepts a gen directory that matches the proto it is generated from", () => {
    // Given
    const gate = aGeneratedCodeRepo();

    // When
    const { exitCode, report } = gate.check();

    // Then
    expect(exitCode).toEqual(0);
    expect(report).toEqual("pkg/gen is up to date.\n");
  }, A_GENERATION_ROUND_TRIP);

  test("rejects a committed file that has been hand-edited away from the proto", () => {
    // Given
    const gate = aGeneratedCodeRepo().withHandEditedGeneratedFile();

    // When
    const { exitCode, report } = gate.check();

    // Then
    expect(exitCode).toEqual(1);
    expect(report.split("\n").slice(0, 2)).toEqual([
      "pkg/gen does not match its protos:",
      "  ~ greeting_pb.ts  stale (+0/-1 lines)",
    ]);
  }, A_GENERATION_ROUND_TRIP);

  test("rejects a generated file that was never committed", () => {
    // Given
    const gate = aGeneratedCodeRepo().withGeneratedFileDeleted();

    // When
    const { exitCode, report } = gate.check();

    // Then
    expect(exitCode).toEqual(1);
    expect(report.split("\n").slice(0, 2)).toEqual([
      "pkg/gen does not match its protos:",
      "  + greeting_pb.ts  generated from the protos, never committed",
    ]);
  }, A_GENERATION_ROUND_TRIP);

  test("rejects a committed file that no proto generates any more", () => {
    // Given
    const gate = aGeneratedCodeRepo().withCommittedFileNoProtoProduces();

    // When
    const { exitCode, report } = gate.check();

    // Then
    expect(exitCode).toEqual(1);
    expect(report.split("\n").slice(0, 2)).toEqual([
      "pkg/gen does not match its protos:",
      "  - retired_pb.ts  committed, but no proto generates it any more",
    ]);
  }, A_GENERATION_ROUND_TRIP);

  test("names the command that regenerates each drifted package", () => {
    // Given
    const gate = aGeneratedCodeRepo().withHandEditedGeneratedFile();

    // When
    const { report } = gate.check();

    // Then
    expect(report.trimEnd().split("\n").slice(-1)).toEqual([
      "  scripts/generated-code.sh write pkg",
    ]);
  }, A_GENERATION_ROUND_TRIP);
});
