/**
 * The "Connect Coder" form: a Coder deployment added by entering its URL and a session token.
 * Signs nobody in.
 *
 * The daemon asks the deployment who the token belongs to before it stores anything, so the form
 * has two outcomes: the account joins the Coder group, or the daemon's reason is shown beside the
 * form and what was typed stays. The token field is a password field and is never read back —
 * `accounts.proto` has no field to return it in.
 */

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

export function ConnectCoderAccountForm(_props: ConnectCoderAccountFormProps) {
  // TODO(coder-account): implement — the URL, token and label fields (`accounts-coder-url`,
  // `accounts-coder-token`, `accounts-coder-label`), the submit control (`accounts-coder-submit`)
  // and the daemon's refusal (`accounts-coder-error`).
  return (
    <form
      data-testid="accounts-coder-form"
      className="mt-4 flex max-w-md flex-col gap-3 text-sm"
      onSubmit={(event) => event.preventDefault()}
    />
  );
}
