/**
 * Acceptance tests: each host row reports whether an ssh-agent is reachable there and which keys it
 * holds.
 *
 * The four states are asserted as mutually distinguishable in one test, not each in isolation — a
 * per-state test would pass even if the component rendered "no agent" and "agent with no keys"
 * identically, which is the bug that sends someone to start an agent that is already running.
 *
 * PRD: docs/ft/web/1-WIP/PRD-2026-09-06-agent-keys.md
 */

import { create } from "@bufbuild/protobuf";
import { anInMemoryRpcBackend } from "tddy-connectrpc-testkit";
import {
  HostSshAgentSchema,
  ProbeOutcome,
  SshAgentKeySchema,
} from "../../src/gen/connection_pb";
import { HostRowSshAgent } from "../../src/components/hosts/HostRowSshAgent";
import { mountWithRpc } from "../support/rpc/inMemory";
import { withSelectedDaemon } from "../support/rpc/withSelectedDaemon";
import { hostSshAgentPage } from "../support/pages/hostsScreenPage";

const HOST = "workstation-1";
const FINGERPRINT = "SHA256:JfISx02kSjWJevGy/MjUdXCv76HaRM3YkYNvepTyHD8";

function mountAgent(sshAgent: ReturnType<typeof create<typeof HostSshAgentSchema>>) {
  mountWithRpc(
    withSelectedDaemon(<HostRowSshAgent instanceId={HOST} sshAgent={sshAgent} />),
    anInMemoryRpcBackend(),
  );
}

describe("Hosts screen ssh-agent", () => {
  it("lists each loaded key with its type fingerprint and comment", () => {
    mountAgent(
      create(HostSshAgentSchema, {
        outcome: ProbeOutcome.OK,
        reachable: true,
        keys: [
          create(SshAgentKeySchema, {
            keyType: "ssh-ed25519",
            fingerprint: FINGERPRINT,
            comment: "ada@workstation",
          }),
        ],
      }),
    );

    hostSshAgentPage.keys(HOST).should("have.length", 1);
    hostSshAgentPage.section(HOST).should("contain.text", "ssh-ed25519");
    hostSshAgentPage.section(HOST).should("contain.text", "ada@workstation");
    // The full fingerprint, not a truncation that two keys could share.
    hostSshAgentPage.section(HOST).should("contain.text", FINGERPRINT);
  });

  it("distinguishes an empty agent from an absent one from a failed probe", () => {
    mountAgent(
      create(HostSshAgentSchema, { outcome: ProbeOutcome.OK, reachable: true, keys: [] }),
    );
    hostSshAgentPage.section(HOST).should("contain.text", "No keys loaded");

    mountAgent(
      create(HostSshAgentSchema, { outcome: ProbeOutcome.OK, reachable: false, keys: [] }),
    );
    hostSshAgentPage.section(HOST).should("contain.text", "No agent");
    hostSshAgentPage.section(HOST).should("not.contain.text", "No keys loaded");

    mountAgent(
      create(HostSshAgentSchema, {
        outcome: ProbeOutcome.FAILED,
        reachable: false,
        failureReason: "agent did not answer",
      }),
    );
    hostSshAgentPage.section(HOST).should("contain.text", "Could not check");
    hostSshAgentPage.section(HOST).should("not.contain.text", "No agent");
  });

  it("presents a key comment as a comment and not as a file path", () => {
    mountAgent(
      create(HostSshAgentSchema, {
        outcome: ProbeOutcome.OK,
        reachable: true,
        keys: [
          create(SshAgentKeySchema, {
            keyType: "ssh-ed25519",
            fingerprint: FINGERPRINT,
            // A comment that *looks* like a path, because they often do. The agent does not know
            // where a key came from, so labelling this as a location would be a fabricated fact.
            comment: "/home/ada/.ssh/id_ed25519",
          }),
        ],
      }),
    );

    hostSshAgentPage.section(HOST).should("contain.text", "/home/ada/.ssh/id_ed25519");
    hostSshAgentPage.section(HOST).should("not.contain.text", "Path");
    hostSshAgentPage.section(HOST).should("not.contain.text", "File");
  });
});
