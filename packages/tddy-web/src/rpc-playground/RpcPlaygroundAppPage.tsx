import { useCallback, useEffect, useMemo, useState } from "react";
import { createClient } from "@connectrpc/connect";
import { create, type Registry } from "@bufbuild/protobuf";
import { createLiveKitTransport } from "tddy-livekit-web";
import {
  ServerReflection,
  ServerReflectionRequestSchema,
} from "../gen/grpc/reflection/v1/reflection_pb";
import { GitHubLoginButton } from "../components/GitHubLoginButton";
import { UserAvatar } from "../components/UserAvatar";
import { DaemonNavMenu } from "../components/shell/DaemonNavMenu";
import { useAuth } from "../hooks/useAuth";
import { useCommonRoom } from "../hooks/useCommonRoom";
import { useRoomParticipants } from "../hooks/useRoomParticipants";
import { presenceIdentityForUser } from "../lib/presenceIdentity";
import { buildRegistry, findMethod } from "./registry";
import { invokeRpc, type InvokeResult } from "./invoke";
import {
  RpcPlaygroundScreen,
  type ServiceInfo,
  type ServiceMethodKind,
} from "./RpcPlaygroundScreen";

const screenShellClassName =
  "min-h-svh w-full min-w-0 box-border px-4 py-6 sm:px-6 font-sans text-foreground";

async function* once<T>(value: T): AsyncIterable<T> {
  yield value;
}

const METHOD_KINDS: ServiceMethodKind[] = [
  "unary",
  "server_streaming",
  "client_streaming",
  "bidi_streaming",
];

function servicesFromRegistry(
  registry: Registry,
  serviceNames: string[],
): ServiceInfo[] {
  const out: ServiceInfo[] = [];
  for (const name of serviceNames) {
    const svc = registry.getService(name);
    if (!svc) continue;
    out.push({
      name,
      methods: svc.methods.map((m) => ({
        name: m.name,
        kind: METHOD_KINDS.includes(m.methodKind as ServiceMethodKind)
          ? (m.methodKind as ServiceMethodKind)
          : "unary",
      })),
    });
  }
  return out;
}

/**
 * RPC Playground shell: joins the common LiveKit room, discovers the services
 * hosted by any selected participant via gRPC ServerReflection, and renders the
 * presentational {@link RpcPlaygroundScreen}.
 *
 * All RPCs (reflection + invocation) go over the existing LiveKit data channel —
 * no HTTP Connect transport is used, so every streaming method kind works.
 */
export function RpcPlaygroundAppPage({
  livekitUrl,
  commonRoom,
  onNavigate,
}: {
  livekitUrl?: string;
  commonRoom?: string;
  onNavigate: (path: string) => void;
}) {
  const { user, isAuthenticated, login, logout, sessionToken } = useAuth();

  const identity = useMemo(
    () => (user ? presenceIdentityForUser(user.login) : undefined),
    [user],
  );

  const { room } = useCommonRoom(
    livekitUrl,
    commonRoom,
    isAuthenticated ? identity : undefined,
  );

  const allParticipants = useRoomParticipants(room);

  const [selectedParticipantId, setSelectedParticipantId] = useState<
    string | null
  >(null);

  // Auto-select the first coder participant (active session / daemon RPC server) when the room populates.
  useEffect(() => {
    if (selectedParticipantId) return;
    const target = allParticipants.find(
      (p) => p.role === "coder" && p.identity !== identity,
    );
    if (target) setSelectedParticipantId(target.identity);
  }, [allParticipants, identity, selectedParticipantId]);

  // LiveKit transport for the selected participant — rebuilt on participant change.
  const transport = useMemo(() => {
    if (!room || !selectedParticipantId) return null;
    return createLiveKitTransport({ room, targetIdentity: selectedParticipantId });
  }, [room, selectedParticipantId]);

  const [registry, setRegistry] = useState<Registry | null>(null);
  const [services, setServices] = useState<ServiceInfo[]>([]);
  const [error, setError] = useState<string | null>(null);

  // Re-run reflection whenever the transport changes (participant switched or room first ready).
  useEffect(() => {
    if (!transport) {
      setRegistry(null);
      setServices([]);
      return;
    }

    let cancelled = false;
    setError(null);

    const reflect = async () => {
      try {
        const reflectionClient = createClient(ServerReflection, transport);

        const listReq = create(ServerReflectionRequestSchema, {
          messageRequest: { case: "listServices", value: "*" },
        });
        const serviceNames: string[] = [];
        for await (const resp of reflectionClient.serverReflectionInfo(
          once(listReq),
        )) {
          if (resp.messageResponse.case === "listServicesResponse") {
            for (const s of resp.messageResponse.value.service) {
              serviceNames.push(s.name);
            }
          }
        }

        const fileProtos: Uint8Array[] = [];
        const seen = new Set<string>();
        for (const name of serviceNames) {
          const symReq = create(ServerReflectionRequestSchema, {
            messageRequest: { case: "fileContainingSymbol", value: name },
          });
          for await (const resp of reflectionClient.serverReflectionInfo(
            once(symReq),
          )) {
            if (resp.messageResponse.case === "fileDescriptorResponse") {
              for (const bytes of resp.messageResponse.value.fileDescriptorProto) {
                const key = bytesKey(bytes);
                if (!seen.has(key)) {
                  seen.add(key);
                  fileProtos.push(bytes);
                }
              }
            }
          }
        }

        if (cancelled) return;

        const reg = buildRegistry(encodeFileDescriptorSet(fileProtos));
        setRegistry(reg);
        setServices(servicesFromRegistry(reg, serviceNames));
      } catch (e) {
        if (!cancelled) {
          setError(e instanceof Error ? e.message : String(e));
          setRegistry(null);
          setServices([]);
        }
      }
    };

    void reflect();
    return () => {
      cancelled = true;
    };
  }, [transport]);

  // Only show participants that serve RPC via LiveKit data channels (role "coder").
  // The "daemon" role is the common-room discovery participant which does not serve LiveKit RPC.
  const participants = useMemo(
    () =>
      allParticipants
        .filter((p) => p.identity !== identity && p.role === "coder")
        .map((p) => ({ id: p.identity, label: p.identity })),
    [allParticipants, identity],
  );

  const handleInvoke = useCallback(
    async (
      serviceName: string,
      methodName: string,
      requestJson: string,
    ): Promise<InvokeResult> => {
      if (!registry || !transport) {
        return {
          kind: "error",
          code: "failed_precondition",
          message: "No participant selected or room not connected.",
        };
      }
      const method = findMethod(registry, serviceName, methodName);
      return invokeRpc(transport, method, requestJson, sessionToken ?? undefined);
    },
    [registry, transport, sessionToken],
  );

  if (!isAuthenticated) {
    return (
      <div className={screenShellClassName}>
        <h1 className="text-2xl font-semibold">tddy-web</h1>
        <p className="mb-4 text-sm text-muted-foreground">
          Sign in with GitHub to access the RPC Playground.
        </p>
        <GitHubLoginButton onClick={login} />
      </div>
    );
  }

  return (
    <div>
      <div className="flex flex-wrap items-center justify-between gap-4 px-4 pt-6 sm:px-6">
        <div className="flex min-w-0 flex-wrap items-center gap-3">
          <DaemonNavMenu onNavigate={onNavigate} />
        </div>
        {user ? <UserAvatar user={user} onLogout={logout} /> : null}
      </div>
      {error ? (
        <p
          className="px-4 text-sm text-destructive sm:px-6"
          data-testid="rpc-playground-error"
        >
          {error}
        </p>
      ) : null}
      <RpcPlaygroundScreen
        services={services}
        participants={participants}
        selectedParticipant={selectedParticipantId ?? undefined}
        onSelectParticipant={setSelectedParticipantId}
        onInvoke={handleInvoke}
        onNavigate={onNavigate}
      />
    </div>
  );
}

function bytesKey(bytes: Uint8Array): string {
  let s = "";
  for (let i = 0; i < bytes.length; i += 1) {
    s += String.fromCharCode(bytes[i]);
  }
  return s;
}

function encodeFileDescriptorSet(fileProtos: Uint8Array[]): Uint8Array {
  const parts: number[] = [];
  for (const proto of fileProtos) {
    parts.push(0x0a);
    appendVarint(parts, proto.length);
    for (let i = 0; i < proto.length; i += 1) {
      parts.push(proto[i]);
    }
  }
  return Uint8Array.from(parts);
}

function appendVarint(out: number[], value: number): void {
  let v = value >>> 0;
  while (v > 0x7f) {
    out.push((v & 0x7f) | 0x80);
    v >>>= 7;
  }
  out.push(v);
}
