/**
 * The "Connect Coder" form: a Coder deployment added by entering its URL and a session token.
 * Signs nobody in.
 *
 * The daemon asks the deployment who the token belongs to before it stores anything, so the form
 * has two outcomes: the account joins the Coder group, or the daemon's reason is shown beside the
 * form and what was typed stays. The token field is a password field and is never read back —
 * `accounts.proto` has no field to return it in.
 */

import { useState } from "react";
import { Button } from "tddy-ui-web";

/** What the form submits; the container turns it into `ConnectCoderAccount`. */
export interface ConnectCoderAccountInput {
  /** The deployment's root, e.g. `https://coder.example.com`. */
  deploymentUrl: string;
  /** A Coder session token for that deployment. Inbound only. */
  token: string;
  label: string;
}

export interface ConnectCoderAccountFormProps {
  /** Set while the daemon is checking the token; the submit control is disabled. */
  pending: boolean;
  /** The daemon's reason the last attempt was refused, verbatim. */
  error: string | null;
  onSubmit: (input: ConnectCoderAccountInput) => void;
  onCancel: () => void;
}

const FIELD_CLASS = "h-8 rounded-md border border-border bg-background px-2 text-sm";

export function ConnectCoderAccountForm({
  pending,
  error,
  onSubmit,
  onCancel,
}: ConnectCoderAccountFormProps) {
  const [deploymentUrl, setDeploymentUrl] = useState("");
  const [token, setToken] = useState("");
  const [label, setLabel] = useState("");

  return (
    <form
      data-testid="accounts-coder-form"
      className="mt-4 flex max-w-md flex-col gap-3 text-sm"
      onSubmit={(event) => {
        event.preventDefault();
        onSubmit({ deploymentUrl, token, label });
      }}
    >
      <label className="flex flex-col gap-1">
        Deployment URL
        <input
          data-testid="accounts-coder-url"
          className={FIELD_CLASS}
          value={deploymentUrl}
          onChange={(event) => setDeploymentUrl(event.target.value)}
        />
      </label>
      <label className="flex flex-col gap-1">
        Label
        <input
          data-testid="accounts-coder-label"
          className={FIELD_CLASS}
          value={label}
          onChange={(event) => setLabel(event.target.value)}
        />
      </label>
      <label className="flex flex-col gap-1">
        Session token
        <input
          data-testid="accounts-coder-token"
          type="password"
          autoComplete="off"
          className={FIELD_CLASS}
          value={token}
          onChange={(event) => setToken(event.target.value)}
        />
      </label>
      {error ? (
        <p data-testid="accounts-coder-error" className="text-destructive">
          {error}
        </p>
      ) : null}
      <div className="flex gap-2">
        <Button type="submit" size="sm" disabled={pending} data-testid="accounts-coder-submit">
          Connect
        </Button>
        <Button type="button" size="sm" variant="ghost" onClick={onCancel}>
          Cancel
        </Button>
      </div>
    </form>
  );
}
