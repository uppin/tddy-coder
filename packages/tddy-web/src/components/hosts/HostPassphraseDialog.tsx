/**
 * Asks the operator for a key passphrase, on behalf of a host that is waiting.
 *
 * Server-initiated, unlike `ScreenSharingPassphraseDialog` — that one is the UI deciding to ask
 * before making a call. Here the host raised the question on `StreamHostPrompts` and is blocked
 * until an answer comes back.
 *
 * The dialog always names the host and shows its public-key fingerprint, so an operator can verify
 * out of band before handing over a secret. A **changed** key blocks submission entirely rather than
 * warning and proceeding.
 */

export interface HostPassphraseDialogProps {
  hostId: string;
  /** What is being unlocked — e.g. the key file name. Never a secret. */
  subject: string;
  /** The host's public-key fingerprint, shown for out-of-band verification. */
  fingerprint: string;
  /** True when this fingerprint differs from the one pinned for this host. */
  keyChanged: boolean;
  onSubmit: (passphrase: string) => void;
  onCancel: () => void;
}

export function HostPassphraseDialog({
  hostId,
  subject,
  fingerprint,
  keyChanged,
  onSubmit,
  onCancel,
}: HostPassphraseDialogProps) {
  // TODO(agent-add-key): implement
  void subject;
  void fingerprint;
  void keyChanged;
  void onSubmit;
  void onCancel;
  return <div data-testid={`host-passphrase-dialog-${hostId}`} />;
}
