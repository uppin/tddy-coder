# tddy-daemon — wiring only

`tddy-daemon` is the daemon's **composition root**: it reads config, builds the services other
crates define, connects them, and runs them. **No non-wiring functionality may be added to this
package.**

## What belongs here

- Config and the daemon's own settings service (`config.rs`, `daemon_settings.rs`,
  `daemon_config_service.rs`).
- Startup, the runtime build stages and shutdown (`main.rs`, `startup.rs`, `runtime.rs`,
  `runtime/`).
- Constructing a service and handing it to another: `Arc::new(...)`, `attach_*(...)`,
  `with_*(...)`, pushing an RPC entry.
- Lifecycle of the child processes the daemon itself spawns, when it holds no session state and
  implements no RPC method.

## What does not

- An RPC method, or any handler body.
- An adapter, port implementation or translation layer, however small — implement the port in the
  crate that defines it or the crate that owns the type (for example
  `impl ProviderAccountsPort for AccountsServiceImpl` lives in `tddy-daemon-rpc`, not here).
- Session state, session logic, or anything that reaches a `SessionHost`.
- Helpers other crates could use.

When a change needs any of these, put it in the library crate that owns the concern (or create
one), and add only the wiring call here. If the owning crate would close a dependency cycle, stop
and ask the developer — do not park the code in the daemon.

`tests/unbundle_endpoint.rs` keeps the module list to the endpoint set; a new module there is a
review question, not a formality.
