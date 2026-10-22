import { useEffect, useState } from "react";
import { create } from "@bufbuild/protobuf";
import type { Client } from "@connectrpc/connect";
import type { AccountsService } from "../gen/accounts_pb";
import { ModelInfoSchema } from "../gen/catalog_pb";
import type { ProviderRouteChoice } from "../components/sessions/CreateSessionProviderField";
import type { AgentModelsState } from "./useAgentModels";

type AccountsClient = Client<typeof AccountsService>;

/** What the daemon says about running one harness on the person's provider accounts. */
export interface ProviderRoutesState {
  harnessRoutable: boolean;
  harnessReason: string;
  choices: ProviderRouteChoice[];
  /** Human-readable error from the daemon, or `null`. Surfaced, never masked. */
  error: string | null;
}

const NO_ROUTES: ProviderRoutesState = {
  harnessRoutable: true,
  harnessReason: "",
  choices: [],
  error: null,
};

/**
 * The accounts `harness` can and cannot run on (`ListProviderRoutes`). Idle — no request — when
 * there is no client or no harness.
 */
export function useProviderRoutes(
  client: AccountsClient | undefined,
  sessionToken: string,
  harness: string | null,
): ProviderRoutesState {
  const [state, setState] = useState<ProviderRoutesState>(NO_ROUTES);

  useEffect(() => {
    if (!client || !harness) {
      setState(NO_ROUTES);
      return;
    }
    let cancelled = false;
    client
      .listProviderRoutes({ sessionToken, harness })
      .then((resp) => {
        if (cancelled) return;
        setState({
          harnessRoutable: resp.harnessRoutable,
          harnessReason: resp.harnessReason,
          choices: resp.options.map((o) => ({
            accountId: o.accountId,
            label: o.label,
            preset: o.preset,
            supported: o.supported,
            reason: o.reason,
          })),
          error: null,
        });
      })
      .catch((err) => {
        if (cancelled) return;
        setState({ ...NO_ROUTES, error: err instanceof Error ? err.message : String(err) });
      });
    return () => {
      cancelled = true;
    };
  }, [client, sessionToken, harness]);

  return state;
}

const IDLE_MODELS: AgentModelsState = { models: [], defaultModel: "", loading: false, error: null };

/**
 * The models one provider account serves (`ListProviderAccountModels`), shaped like the vendor
 * catalog so the form's model field renders either. Idle while no account is chosen.
 */
export function useProviderAccountModels(
  client: AccountsClient | undefined,
  sessionToken: string,
  accountId: string,
): AgentModelsState {
  const [state, setState] = useState<AgentModelsState>(IDLE_MODELS);

  useEffect(() => {
    if (!client || !accountId) {
      setState(IDLE_MODELS);
      return;
    }
    let cancelled = false;
    setState({ ...IDLE_MODELS, loading: true });
    client
      .listProviderAccountModels({ sessionToken, accountId })
      .then((resp) => {
        if (cancelled) return;
        setState({
          models: resp.modelIds.map((id) => create(ModelInfoSchema, { id, label: id })),
          defaultModel: resp.modelIds[0] ?? "",
          loading: false,
          error: null,
        });
      })
      .catch((err) => {
        if (cancelled) return;
        setState({
          ...IDLE_MODELS,
          error: err instanceof Error ? err.message : String(err),
        });
      });
    return () => {
      cancelled = true;
    };
  }, [client, sessionToken, accountId]);

  return state;
}

/**
 * The provider account the person made their default for `harness` (`ListAccounts`'s `defaultFor`),
 * or `null` while unknown, when there is none, or when the listing failed — a failure here only
 * means nothing is pre-selected; the choice stays the person's.
 */
export function useProviderDefault(
  client: AccountsClient | undefined,
  sessionToken: string,
  harness: string | null,
): string | null {
  const [found, setFound] = useState<{ harness: string; accountId: string | null } | null>(null);

  useEffect(() => {
    if (!client || !harness) {
      setFound(null);
      return;
    }
    let cancelled = false;
    client
      .listAccounts({ sessionToken })
      .then((resp) => {
        if (cancelled) return;
        const holder = resp.providers
          .flatMap((group) => group.accounts)
          .find((account) => account.defaultFor.includes(harness));
        setFound({ harness, accountId: holder?.accountId ?? null });
      })
      .catch(() => {
        if (!cancelled) setFound({ harness, accountId: null });
      });
    return () => {
      cancelled = true;
    };
  }, [client, sessionToken, harness]);

  return found !== null && found.harness === harness ? found.accountId : null;
}
