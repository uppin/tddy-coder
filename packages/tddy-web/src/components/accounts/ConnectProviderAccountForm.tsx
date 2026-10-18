/**
 * The "Connect API key" form: a model-provider account (Meta, Fireworks, Baseten, Modal, or a
 * compatible endpoint) added by pasting its key. Signs nobody in.
 *
 * A preset whose endpoint the operator supplies asks for that URL; a fixed preset does not show the
 * field. The key field is a password field and is never read back — `accounts.proto` has no field
 * to return it in.
 */

/** One preset as the form offers it. */
export interface ProviderPresetOption {
  id: string;
  label: string;
  /** The preset needs an OpenAI-compatible root from the operator. */
  needsOpenAiBaseUrl: boolean;
  /** The preset needs an Anthropic Messages root from the operator. */
  needsAnthropicBaseUrl: boolean;
}

/** What the form submits; the container turns it into `ConnectProviderAccount`. */
export interface ConnectProviderAccountInput {
  preset: string;
  label: string;
  apiKey: string;
  openAiBaseUrl: string;
  anthropicBaseUrl: string;
}

export interface ConnectProviderAccountFormProps {
  presets: ProviderPresetOption[];
  /** Set while the daemon is checking the key; the submit control is disabled. */
  pending: boolean;
  /** The daemon's reason the last attempt was refused, verbatim. */
  error: string | null;
  onSubmit: (input: ConnectProviderAccountInput) => void;
  onCancel: () => void;
}

export function ConnectProviderAccountForm(props: ConnectProviderAccountFormProps) {
  void props;
  // TODO(accounts): implement
  return null;
}
