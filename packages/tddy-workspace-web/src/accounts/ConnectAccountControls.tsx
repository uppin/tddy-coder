/**
 * The two "connect" controls above the account groups: each is a button that opens its form and
 * closes it again once the daemon has added the account. `AccountsContent` owns the RPCs behind
 * them; these own only whether the form is open and whether a connect is in flight.
 */

import { useState } from "react";
import { Button } from "tddy-ui-web";
import {
  ConnectProviderAccountForm,
  PROVIDER_PRESET_OPTIONS,
  type ConnectProviderAccountInput,
} from "./ConnectProviderAccountForm";
import { ConnectCoderAccountForm, type ConnectCoderAccountInput } from "./ConnectCoderAccountForm";

/** "Connect Coder" and, once pressed, its form; closes when the daemon added the account. */
export function ConnectCoder({
  onConnect,
  error,
}: {
  onConnect: (input: ConnectCoderAccountInput) => Promise<boolean>;
  error: string | null;
}) {
  const [connecting, setConnecting] = useState(false);
  const [pending, setPending] = useState(false);
  return (
    <div>
      <Button
        size="sm"
        variant="outline"
        data-testid="accounts-connect-coder"
        onClick={() => setConnecting(true)}
      >
        Connect Coder
      </Button>
      {connecting ? (
        <ConnectCoderAccountForm
          pending={pending}
          error={error}
          onCancel={() => setConnecting(false)}
          onSubmit={(input) => {
            setPending(true);
            onConnect(input)
              .then((added) => {
                if (added) setConnecting(false);
              })
              .finally(() => setPending(false));
          }}
        />
      ) : null}
    </div>
  );
}

/** "Connect API key" and, once pressed, its form; closes when the daemon added the account. */
export function ConnectApiKey({
  onConnect,
  error,
}: {
  onConnect: (input: ConnectProviderAccountInput) => Promise<boolean>;
  error: string | null;
}) {
  const [connecting, setConnecting] = useState(false);
  const [pending, setPending] = useState(false);
  return (
    <div className="mb-4">
      <Button
        size="sm"
        variant="outline"
        data-testid="accounts-connect-api-key"
        onClick={() => setConnecting(true)}
      >
        Connect API key
      </Button>
      {connecting ? (
        <ConnectProviderAccountForm
          presets={PROVIDER_PRESET_OPTIONS}
          pending={pending}
          error={error}
          onCancel={() => setConnecting(false)}
          onSubmit={(input) => {
            setPending(true);
            onConnect(input)
              .then((added) => {
                if (added) setConnecting(false);
              })
              .finally(() => setPending(false));
          }}
        />
      ) : null}
    </div>
  );
}
