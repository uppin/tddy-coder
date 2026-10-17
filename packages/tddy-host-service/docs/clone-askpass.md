# Clone askpass (tddy-host-service)

The daemon's end of `tddy-askpass`: the loopback endpoint a clone's ssh reaches, through its helper, when
it needs a key passphrase. Product behaviour:
[cloning over SSH](../../../docs/ft/daemon/clone-ssh-passphrase.md). The same prompt registry and host
keypair as [`AddHostKey`](host-add-key.md) carry the question; this module is a second *trigger* for it.

## The modules (`src/askpass/`)

| Module | Holds |
|---|---|
| `grants.rs` | `CloneGrants` (the live tokens), `CloneGrant` (one clone's token, revoked on drop) |
| `endpoint.rs` | `AskpassEndpoint`: binds the loopback listener and decides each request |
| `http.rs` | the one route, `POST /rpc/askpass.AskpassService/Prompt` |
| `outcome.rs` | `AskpassOutcome`, `AskpassReply`, `Passphrase` |

## One clone, one grant

`AskpassEndpoint::register_clone(github_user)` returns a `CloneGrant` whose `token()` is 64 hex characters
(two v4 UUIDs) and whose `url()` is the endpoint's `http://127.0.0.1:<port>`. The grant lives for the
clone: dropping it removes the token, so a helper that outlives its clone is refused. The token is per
clone and not per question because ssh asks up to three times for one key, all with the same environment.

`CloneGrants::new()` builds grants that name no endpoint (`url()` is empty); grants a helper can use come
from `register_clone`.

## Deciding a request

`AskpassEndpoint::ask(token, prompt_text)` (the in-process entry; the route calls the same core):

1. An unknown or revoked token is `Refused`. No prompt is raised.
2. The prompt text is classified by `tddy_askpass::prompt_text::classify`. Anything but
   `Enter passphrase for key '<path>': ` is `Refused`, so a host-key or password question never reaches an
   operator.
3. A `PromptKind::SshKeyPassphrase` prompt is issued for the token's GitHub user, with the key path as its
   subject, and its answer handoff is claimed immediately — issuing puts it on the feed, so a browser may
   answer at once.
4. The wait is the shorter of the prompt's own expiry and `answer_window` (the prompt's 120-second
   lifetime by default; shorter only in tests). No answer is `Expired`.
5. The ciphertext is decrypted by the host keypair on the blocking pool and returned as
   `AskpassOutcome::Answer(Passphrase)`. A failed decrypt is `Refused` with a reason that holds no secret.

`ask_reporting_rejection` returns an `AskpassReply` carrying, with the outcome, whether this token already
received an answer for the same key. That is ssh's retry after a wrong passphrase, and the helper turns it
into `the previous passphrase for key '…' was rejected (incorrect passphrase)` on its stderr. The grant
remembers key paths only, never passphrases.

## The passphrase

`Passphrase` holds the plaintext until dropped, zeroes it with volatile writes on drop, compares with
`&[u8]`, and prints as `Passphrase(<redacted>)`. Nothing here logs an answer, a key path or a prompt text;
the log carries outcomes only.

## The wire

`POST /rpc/askpass.AskpassService/Prompt` (`tddy_service::proto::askpass::PROMPT_PATH`), Connect's
`/rpc/{service}/{method}` shape with a prost body, `content-type: application/proto`. `AskpassRequest`
carries `token` and `prompt`; `AskpassResponse` carries `outcome`, `reason`, `previous_answer_rejected` and
`answer`, and `answer` is filled only for `OUTCOME_ANSWER`. A malformed body is HTTP 400 with a Connect-style
JSON error. The listener binds `127.0.0.1` only. Messages are in `packages/tddy-service/proto/askpass.proto`
(messages only; the route is served by hand).

## Wiring

`tddy-daemon`'s runtime starts one `AskpassEndpoint` on the `host_prompts` registry and the keypair
`HostServiceImpl` publishes, and installs it on the session host (`with_askpass_endpoint`), whence it
reaches `LaunchSessions.askpass`. It has to be those instances: a prompt raised on another registry would
never reach `StreamHostPrompts`. A host not built by the daemon runtime has none, and its SSH clones are
given no helper.

## Testing

`tests/askpass_prompt_rpc.rs` (the prompt is raised for the clone's operator; another operator never sees
it and cannot answer it; an unanswered prompt ends as expired at its deadline; non-passphrase text, an
unknown token and a token whose clone ended are refused without a prompt; the answer is in no log line or
`Debug` output; the retry flag), `tests/clone_grants.rs` (tokens name their operator, die with their clone,
are long and distinct) and, against the real endpoint, `tddy-askpass/tests/askpass_cli.rs`. The real
`git`, `ssh`, `sshd` and `ssh-agent` run in `tddy-agent-launch/tests/clone_askpass_acceptance.rs`.

## See also

- [host-add-key.md](host-add-key.md) — the prompt registry, the keypair and the answer's encryption
- [`packages/tddy-askpass/README.md`](../../tddy-askpass/README.md) — the helper
- [`packages/tddy-spawn/docs/clone-ssh-environment.md`](../../tddy-spawn/docs/clone-ssh-environment.md) — the
  environment a clone is given
