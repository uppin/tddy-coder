//! No RPC in the screen-sharing schema carries a passphrase.
//!
//! `#keyring` 7/9 deletes `UnlockVault` — the call whose request held a second secret, typed by a
//! person and sent over a wire. It existed only because the daemon had no other way to know the
//! person was present; after 2/9 and 3/9 it does, and a target's password is a record the caller's
//! own session opens.
//!
//! The property is asserted **over the schema** rather than over this crate's generated Rust,
//! following `service_coordinates.rs`: the `.proto` is what a client in any language is generated
//! from, so a passphrase reintroduced there reaches browsers whether or not any Rust reads it.
//! Asserting it as a test rather than as a review criterion is the point — a reintroduction by a
//! merge, a revert or a well-meant "just for the migration" fails here instead of passing quietly.
//!
//! **Scoped to `screen_sharing.proto` on purpose.** `host.proto` carries an *ssh key's* passphrase,
//! which is a different secret with a different owner, and a workspace-wide scan would report it as
//! a violation of a rule it was never under.

use std::collections::HashMap;
use std::path::Path;

const SCHEMA: &str = "screen_sharing.proto";

fn the_schema() -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("proto")
        .join(SCHEMA);
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{} is readable: {e}", path.display()))
}

/// Every `rpc Name (Request) returns (Response);` the schema's single service declares, as
/// `(method, request type, response type)`.
fn the_rpcs_of(schema: &str) -> Vec<(String, String, String)> {
    schema
        .lines()
        .map(str::trim)
        .filter_map(|line| {
            let rest = line.strip_prefix("rpc ")?;
            let (name, rest) = rest.split_once('(')?;
            let (request, rest) = rest.split_once(')')?;
            let (_, rest) = rest.split_once('(')?;
            let (response, _) = rest.split_once(')')?;
            Some((
                name.trim().to_string(),
                request.trim().to_string(),
                response.trim().to_string(),
            ))
        })
        .collect()
}

/// Every `message Name { … }` the schema declares, as name → its field names.
fn the_fields_of_each_message_in(schema: &str) -> HashMap<String, Vec<String>> {
    let mut messages = HashMap::new();
    let mut open: Option<(String, Vec<String>)> = None;

    for line in schema.lines().map(str::trim) {
        match open.take() {
            None => {
                if let Some(name) = line
                    .strip_prefix("message ")
                    .and_then(|r| r.strip_suffix(" {"))
                {
                    open = Some((name.trim().to_string(), Vec::new()));
                }
            }
            Some((name, mut fields)) => {
                if line == "}" {
                    messages.insert(name, fields);
                } else {
                    // `<type> <name> = <number>;`, with `repeated`/`optional` ahead of the type.
                    if let Some((declaration, _)) = line.split_once('=') {
                        if let Some(field) = declaration.split_whitespace().last() {
                            fields.push(field.to_string());
                        }
                    }
                    open = Some((name, fields));
                }
            }
        }
    }

    messages
}

#[test]
fn no_rpc_in_the_screen_sharing_schema_carries_a_passphrase() {
    // Given every message reachable as an RPC's request or response
    let schema = the_schema();
    let fields = the_fields_of_each_message_in(&schema);
    let rpcs = the_rpcs_of(&schema);
    assert!(!rpcs.is_empty(), "{SCHEMA} declares at least one rpc");

    // When collecting the ones whose payload names a passphrase
    let carrying: Vec<String> = rpcs
        .iter()
        .flat_map(|(method, request, response)| {
            [(method, request), (method, response)]
                .into_iter()
                .filter(|(_, message)| {
                    fields
                        .get(*message)
                        .is_some_and(|names| names.iter().any(|name| name.contains("passphrase")))
                })
                .map(|(method, message)| format!("{method} ({message})"))
                .collect::<Vec<_>>()
        })
        .collect();

    // Then there are none, and there must never be one again
    assert_eq!(
        carrying,
        Vec::<String>::new(),
        "an rpc in {SCHEMA} carries a passphrase. A target's password is a `screen-sharing` \
         record the caller's own session opens; a second secret on the wire is the thing \
         `#keyring` 7/9 removed"
    );
}

#[test]
fn the_screen_sharing_schema_declares_no_unlock_rpc() {
    // Given the schema
    let schema = the_schema();

    // When reading the methods it declares
    let methods: Vec<String> = the_rpcs_of(&schema)
        .into_iter()
        .map(|(method, _, _)| method)
        .collect();

    // Then none of them unlocks anything — there is nothing to unlock
    let unlocking: Vec<&String> = methods
        .iter()
        .filter(|method| method.to_lowercase().contains("unlock"))
        .collect();
    assert_eq!(
        unlocking,
        Vec::<&String>::new(),
        "{SCHEMA} declares an unlock rpc. The session is the key: a store that needs a separate \
         unlock call is one a daemon can hold open with nobody present"
    );
}
