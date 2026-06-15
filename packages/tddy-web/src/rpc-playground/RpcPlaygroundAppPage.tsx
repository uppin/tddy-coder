import { useCallback, useEffect, useMemo, useState } from "react";
import { createClient, type Transport } from "@connectrpc/connect";
import { createConnectTransport } from "@connectrpc/connect-web";
import { create, type Registry } from "@bufbuild/protobuf";
import {
  ServerReflection,
  ServerReflectionRequestSchema,
} from "../gen/grpc/reflection/v1/reflection_pb";
import { GitHubLoginButton } from "../components/GitHubLoginButton";
import { UserAvatar } from "../components/UserAvatar";
import { DaemonNavMenu } from "../components/shell/DaemonNavMenu";
import { useAuth } from "../hooks/useAuth";
import { buildRegistry, findMethod } from "./registry";
import { invokeRpc, type InvokeResult } from "./invoke";
import {
  RpcPlaygroundScreen,
  type ServiceInfo,
  type ServiceMethodKind,
} from "./RpcPlaygroundScreen";

const screenShellClassName =
  "min-h-svh w-full min-w-0 box-border px-4 py-6 sm:px-6 font-sans text-foreground";

function createRpcTransport(): Transport {
  return createConnectTransport({
    baseUrl:
      typeof window !== "undefined" ? `${window.location.origin}/rpc` : "",
    useBinaryFormat: true,
  });
}

/** Drive the ServerReflection bidi stream with a single request and collect responses. */
async function* once<T>(value: T): AsyncIterable<T> {
  yield value;
}

const METHOD_KINDS: ServiceMethodKind[] = [
  "unary",
  "server_streaming",
  "client_streaming",
  "bidi_streaming",
];

/** Map a registry into the presentational ServiceInfo[] shape. */
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
 * RPC Playground shell: authenticates, reflects the hosted services over the local
 * `/rpc` Connect transport, and renders the presentational {@link RpcPlaygroundScreen}.
 */
export function RpcPlaygroundAppPage({
  onNavigate,
}: {
  onNavigate: (path: string) => void;
}) {
  const { user, isAuthenticated, login, logout } = useAuth();
  const transport = useMemo(() => createRpcTransport(), []);
  const reflectionClient = useMemo(
    () => createClient(ServerReflection, transport),
    [transport],
  );

  const [registry, setRegistry] = useState<Registry | null>(null);
  const [services, setServices] = useState<ServiceInfo[]>([]);
  const [error, setError] = useState<string | null>(null);

  const reflect = useCallback(async () => {
    setError(null);
    try {
      // 1. List the hosted services.
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

      // 2. Fetch descriptors for each service and accumulate file descriptor protos.
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

      // 3. Build a FileDescriptorSet and registry from the collected protos.
      const reg = buildRegistry(encodeFileDescriptorSet(fileProtos));
      setRegistry(reg);
      setServices(servicesFromRegistry(reg, serviceNames));
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
      setRegistry(null);
      setServices([]);
    }
  }, [reflectionClient]);

  useEffect(() => {
    if (!isAuthenticated) return;
    void reflect();
  }, [isAuthenticated, reflect]);

  const handleInvoke = useCallback(
    async (
      serviceName: string,
      methodName: string,
      requestJson: string,
    ): Promise<InvokeResult> => {
      if (!registry) {
        return {
          kind: "error",
          code: "failed_precondition",
          message: "Reflection registry not loaded yet.",
        };
      }
      const method = findMethod(registry, serviceName, methodName);
      return invokeRpc(transport, method, requestJson);
    },
    [registry, transport],
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
        onInvoke={handleInvoke}
        onNavigate={onNavigate}
      />
    </div>
  );
}

function bytesKey(bytes: Uint8Array): string {
  // Cheap content key for de-duplication of file descriptor protos.
  let s = "";
  for (let i = 0; i < bytes.length; i += 1) {
    s += String.fromCharCode(bytes[i]);
  }
  return s;
}

/**
 * Encode a list of serialized `FileDescriptorProto` bytes as a serialized
 * `FileDescriptorSet` (a single repeated field #1 of LEN-delimited entries).
 */
function encodeFileDescriptorSet(fileProtos: Uint8Array[]): Uint8Array {
  const parts: number[] = [];
  for (const proto of fileProtos) {
    parts.push(0x0a); // field 1, wire type 2 (LEN)
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
