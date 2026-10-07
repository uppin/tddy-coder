//! Linking an account hands back no session.
//!
//! `#keyring` 8/9 adds the two RPCs that put a *second* GitHub account into a person's vault. The
//! convenient way to build them is to call 2/9's login flow and throw away the session it returns —
//! which works, passes every positive test anyone would write, and silently re-identifies the caller
//! as whoever just approved at GitHub. A person adding their work account would find themselves
//! signed in as it, looking at a different vault.
//!
//! The property is asserted **over the schema** rather than over generated Rust, following
//! `service_coordinates.rs` and `screen_sharing_carries_no_passphrase.rs`: the `.proto` is what
//! clients in every language are generated from, so a token field reintroduced there reaches a
//! browser whether or not any Rust reads it. A schema has no field to put a session in, and no
//! implementation can then return one by accident.
//!
//! Two files are read because the boundary has two halves — the linking messages carry no token
//! (`accounts.proto`), and the flow that *does* mint tokens grew no way to link (`auth.proto`).

use std::collections::HashMap;
use std::path::Path;

const ACCOUNTS: &str = "accounts.proto";
const AUTH: &str = "auth.proto";

/// Field names that hand the caller an identity. `session_token` and `refresh_token` are what
/// `auth.proto` actually returns; the rest are the names a well-meant shortcut would reach for.
const NAMES_THAT_CARRY_AN_IDENTITY: [&str; 6] = [
    "session_token",
    "refresh_token",
    "access_token",
    "id_token",
    "token",
    "jwt",
];

fn the_schema(name: &str) -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("proto")
        .join(name);
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{} is readable: {e}", path.display()))
}

/// Every `rpc Name (Request) returns (Response);` a schema declares, as
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

/// Every `message Name { … }` a schema declares, as name → its field names.
///
/// This parser reads one declaration per line, which is all `accounts.proto` needs. It **refuses**
/// the shapes it cannot read — a nested `message`/`oneof`/`enum`, a `}` sharing a line with other
/// text, a one-line `message X {}` — rather than skipping them: a skipped nested message would hide
/// its token fields from the assertions and the test would pass without having looked.
fn the_fields_of_each_message_in(schema: &str) -> Result<HashMap<String, Vec<String>>, String> {
    let mut messages = HashMap::new();
    let mut open: Option<(String, Vec<String>)> = None;

    for line in schema.lines().map(without_comment).map(str::trim) {
        match open.take() {
            None => {
                if line.starts_with("message ") {
                    let name = line
                        .strip_prefix("message ")
                        .and_then(|r| r.strip_suffix(" {"))
                        .ok_or_else(|| {
                            format!("a message declaration this parser cannot read: `{line}`")
                        })?;
                    open = Some((name.trim().to_string(), Vec::new()));
                }
            }
            Some((name, mut fields)) => {
                if line == "}" {
                    messages.insert(name, fields);
                    continue;
                }
                if line.contains('{') || line.contains('}') {
                    return Err(format!(
                        "`{line}` inside message {name} is a shape this parser cannot read \
                         (nested message, oneof, enum, or an inline `}}`)"
                    ));
                }
                // `<type> <name> = <number>;`, with `repeated`/`optional` ahead of the type.
                if let Some(field) = line
                    .split_once('=')
                    .and_then(|(declaration, _)| declaration.split_whitespace().last())
                {
                    fields.push(field.to_string());
                }
                open = Some((name, fields));
            }
        }
    }

    match open {
        Some((name, _)) => Err(format!("message {name} is never closed")),
        None => Ok(messages),
    }
}

fn without_comment(line: &str) -> &str {
    line.split_once("//").map_or(line, |(code, _)| code)
}

fn the_messages_of(schema_name: &str) -> HashMap<String, Vec<String>> {
    the_fields_of_each_message_in(&the_schema(schema_name))
        .unwrap_or_else(|reason| panic!("{schema_name}: {reason}"))
}

fn the_linking_rpcs_of(schema: &str) -> Vec<(String, String, String)> {
    the_rpcs_of(schema)
        .into_iter()
        .filter(|(method, _, _)| method.to_lowercase().contains("link"))
        .collect()
}

#[test]
fn no_response_to_a_link_carries_anything_that_would_sign_the_caller_in() {
    // Given the linking rpcs and every message they name
    let schema = the_schema(ACCOUNTS);
    let fields = the_messages_of(ACCOUNTS);
    let linking = the_linking_rpcs_of(&schema);
    assert_eq!(
        linking.len(),
        2,
        "{ACCOUNTS} declares BeginLinkAccount and PollLinkAccount"
    );

    // When collecting the responses whose fields would identify whoever reads them
    let carrying: Vec<String> = linking
        .iter()
        .filter(|(_, _, response)| {
            fields.get(response.as_str()).is_some_and(|names| {
                names
                    .iter()
                    .any(|name| NAMES_THAT_CARRY_AN_IDENTITY.contains(&name.as_str()))
            })
        })
        .map(|(method, _, response)| format!("{method} ({response})"))
        .collect();

    // Then there are none: adding a credential is not becoming its owner
    assert_eq!(
        carrying,
        Vec::<String>::new(),
        "a link response in {ACCOUNTS} hands back a token. Completing an authorization dance is \
         how `auth.proto` establishes who someone is — a link that returns one signs the caller in \
         as the account they just added, and they lose the vault they were looking at"
    );
}

#[test]
fn the_request_that_begins_a_link_is_authorized_by_the_session_already_in_hand() {
    // Given the linking rpcs and every message they name
    let schema = the_schema(ACCOUNTS);
    let fields = the_messages_of(ACCOUNTS);
    let linking = the_linking_rpcs_of(&schema);

    // When reading what each one asks the caller for
    let without_a_session: Vec<String> = linking
        .iter()
        .filter(|(_, request, _)| {
            !fields
                .get(request.as_str())
                .is_some_and(|names| names.iter().any(|name| name == "session_token"))
        })
        .map(|(method, request, _)| format!("{method} ({request})"))
        .collect();

    // Then both are asked in the name of a session that already exists
    assert_eq!(
        without_a_session,
        Vec::<String>::new(),
        "a link rpc in {ACCOUNTS} takes no session token. The vault a linked account goes into is \
         the caller's own, and an unauthenticated link would have no vault to write to"
    );
}

#[test]
fn the_listing_names_the_account_the_session_belongs_to() {
    // Given the listing's response
    let fields = the_messages_of(ACCOUNTS);

    // When reading where the session's own account is named
    let listing = fields
        .get("ListAccountsResponse")
        .expect("accounts.proto declares ListAccountsResponse");

    // Then it is a fact about the caller, stated on the response
    assert!(
        listing.contains(&"session_account".to_string()),
        "ListAccountsResponse does not say which account the session was established with, so a \
         screen cannot mark the one `RemoveAccount` refuses to forget"
    );
}

#[test]
fn an_account_row_carries_no_session_flag() {
    // Given one account row
    let fields = the_messages_of(ACCOUNTS);
    let row = fields
        .get("AccountSummary")
        .expect("accounts.proto declares AccountSummary");

    // When reading the row's fields for anything about the session
    let about_the_session: Vec<&String> =
        row.iter().filter(|name| name.contains("session")).collect();

    // Then there is nothing: the bit belongs to the caller, not to a row every caller shares
    assert_eq!(
        about_the_session,
        Vec::<&String>::new(),
        "AccountSummary carries a session bit. The same record is the session's account on one \
         daemon and an ordinary linked account on another once 6/9 propagates it — putting the \
         bit on the row makes the row's meaning depend on who read it"
    );
}

// ---------------------------------------------------------------------------------------------
// The parser's own limits

#[test]
fn a_nested_message_is_refused_rather_than_skipped() {
    // Given a message that declares another inside it
    let schema = "message Outer {\n  message Inner {\n    string token = 1;\n  }\n}\n";

    // When parsing it
    let parsed = the_fields_of_each_message_in(schema);

    // Then the parser says it cannot read it
    assert!(parsed.is_err(), "parsed to {parsed:?}");
}

#[test]
fn a_oneof_is_refused_rather_than_skipped() {
    // Given a message whose token hides inside a oneof
    let schema = "message Reply {\n  oneof who {\n    string session_token = 1;\n  }\n}\n";

    // When parsing it
    let parsed = the_fields_of_each_message_in(schema);

    // Then the parser says it cannot read it
    assert!(parsed.is_err(), "parsed to {parsed:?}");
}

#[test]
fn a_closing_brace_sharing_a_line_is_refused_rather_than_skipped() {
    // Given a message closed on the same line as its last field
    let schema = "message Reply {\n  string session_token = 1; }\n";

    // When parsing it
    let parsed = the_fields_of_each_message_in(schema);

    // Then the parser says it cannot read it
    assert!(parsed.is_err(), "parsed to {parsed:?}");
}

#[test]
fn a_plain_message_yields_its_field_names() {
    // Given a message with a repeated and an optional field
    let schema = "message Reply {\n  string name = 1; // who\n  repeated string tags = 2;\n}\n";

    // When parsing it
    let parsed = the_fields_of_each_message_in(schema).expect("a readable schema");

    // Then every field name is there
    assert_eq!(
        parsed.get("Reply"),
        Some(&vec!["name".to_string(), "tags".to_string()])
    );
}

#[test]
fn the_flow_that_mints_sessions_grew_no_way_to_link_an_account() {
    // Given the schema that establishes identity
    let schema = the_schema(AUTH);
    let methods: Vec<String> = the_rpcs_of(&schema)
        .into_iter()
        .map(|(method, _, _)| method)
        .collect();
    assert!(!methods.is_empty(), "{AUTH} declares at least one rpc");

    // When reading the methods it declares
    let linking: Vec<&String> = methods
        .iter()
        .filter(|method| method.to_lowercase().contains("link"))
        .collect();

    // Then none of them links an account — the separation is which service a method lives on
    assert_eq!(
        linking,
        Vec::<&String>::new(),
        "{AUTH} declares a linking rpc. The two flows are told apart by the service they live on; \
         a link served beside the login is one refactor away from sharing its session-minting path"
    );
}
