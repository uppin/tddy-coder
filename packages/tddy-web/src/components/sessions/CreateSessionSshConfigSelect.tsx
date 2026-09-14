/**
 * Create-session SSH Host alias picker.
 *
 * Empty value is LocalShell (this host). A listed alias is RemoteShell on that OpenSSH Host.
 * The list is `ListSshConfigHosts` addressed at {@link sshConfigListDaemonId}: the session host
 * when co-located, the codebase host when split (n4). Honesty: a failed read is never rendered
 * as an empty alias list.
 */

import { useEffect, useState } from "react";
import { create } from "@bufbuild/protobuf";
import { Code, ConnectError } from "@connectrpc/connect";
import {
  HostService,
  ListSshConfigHostsRequestSchema,
  ProbeOutcome,
} from "../../gen/host_pb";
import { useHostClient, useHostConnection } from "../../rpc/connections/registry";
import { useLiveKitTransportFactoryIsOverridden } from "../../rpc/transportProvider";
import { inputClass, labelClass } from "./createSessionFormStyles";

/**
 * Daemon whose `~/.ssh/config` the create-session SSH dropdown lists.
 *
 * Co-located: the session host (A) is the OpenSSH client.
 * Split: the codebase host (B) holds the checkout and opens `ssh(1)`.
 */
export function sshConfigListDaemonId(
  sessionHostDaemonId: string,
  codebaseHostDaemonId: string,
): string {
  const split =
    codebaseHostDaemonId !== "" && codebaseHostDaemonId !== sessionHostDaemonId;
  // TODO(split): when `split`, return `codebaseHostDaemonId` — that daemon is the OpenSSH client.
  void split;
  return sessionHostDaemonId;
}

type LoadState =
  | { kind: "idle" }
  | { kind: "loading" }
  | { kind: "loaded"; aliases: string[] }
  | { kind: "failed"; message: string };

export function CreateSessionSshConfigSelect({
  sessionToken,
  listDaemonInstanceId,
  value,
  onChange,
}: {
  sessionToken: string;
  listDaemonInstanceId: string;
  value: string;
  onChange: (sshConfigHost: string) => void;
}) {
  const connection = useHostConnection(listDaemonInstanceId || null);
  const hostClient = useHostClient(HostService, listDaemonInstanceId || null);
  // A production LiveKit transport needs a connected Room; the in-memory test factory does not
  // (`useLiveKitTransportFactoryIsOverridden`). Without this guard, a cy.mount CreateSessionPane
  // would open a real LiveKit client against a disconnected fixture Room.
  const factoryOverridden = useLiveKitTransportFactoryIsOverridden();
  const hostReachable =
    hostClient !== null &&
    connection !== null &&
    (connection.status === "connected" || factoryOverridden);
  const [load, setLoad] = useState<LoadState>({ kind: "idle" });

  useEffect(() => {
    if (!hostReachable || !hostClient || !listDaemonInstanceId) {
      setLoad({ kind: "idle" });
      return;
    }
    let cancelled = false;
    setLoad({ kind: "loading" });
    void hostClient
      .listSshConfigHosts(
        create(ListSshConfigHostsRequestSchema, {
          sessionToken,
          daemonInstanceId: listDaemonInstanceId,
        }),
      )
      .then((res) => {
        if (cancelled) return;
        switch (res.outcome) {
          case ProbeOutcome.OK:
            setLoad({ kind: "loaded", aliases: res.hosts.map((h) => h.alias) });
            return;
          case ProbeOutcome.UNSPECIFIED:
            setLoad({ kind: "failed", message: "Could not list SSH hosts" });
            return;
          case ProbeOutcome.UNSUPPORTED:
            setLoad({
              kind: "failed",
              message: res.failureReason || "This host cannot list SSH config aliases",
            });
            return;
          // FAILED, and every outcome a newer daemon may add that this bundle cannot name.
          default:
            setLoad({
              kind: "failed",
              message: res.failureReason || "Could not list SSH hosts",
            });
        }
      })
      .catch((err: unknown) => {
        if (cancelled) return;
        const connect = ConnectError.from(err);
        setLoad({
          kind: "failed",
          message:
            connect.code === Code.FailedPrecondition
              ? connect.rawMessage
              : connect.message,
        });
      });
    return () => {
      cancelled = true;
    };
  }, [hostReachable, hostClient, listDaemonInstanceId, sessionToken]);

  const aliases = load.kind === "loaded" ? load.aliases : [];

  return (
    <div>
      <label className={labelClass} htmlFor="create-session-ssh-config">
        SSH host
      </label>
      {load.kind === "failed" ? (
        <p data-testid="create-session-ssh-config-error">{load.message}</p>
      ) : (
        <select
          id="create-session-ssh-config"
          data-testid="create-session-ssh-config-select"
          className={inputClass}
          value={value}
          onChange={(e) => onChange(e.target.value)}
        >
          <option value="">This host</option>
          {aliases.map((alias) => (
            <option key={alias} value={alias}>
              {alias}
            </option>
          ))}
        </select>
      )}
    </div>
  );
}
