//! Acceptance: the conversation tree stops at its depth guard and never revisits a file, whatever
//! a binding's `children` template points at (`#usage-binding 1/5`).

mod support;

use pretty_assertions::assert_eq;
use support::{an_agent_home, usage};
use tddy_usage::{account, UsageSection};

/// A binding whose children live beside their parent as `<stem>.child.jsonl` — so every child has
/// a child of its own, as deep as the files go.
fn a_binding_with_children(children_path: &str) -> UsageSection {
    serde_yaml::from_str(&format!(
        r#"
sources:
  - format: jsonl
    main: {{ path: "{{agent_home}}/{{conversation_id}}.jsonl" }}
    children: {{ path: "{children_path}", id: "{{stem}}" }}
    record:
      where: [{{ field: type, equals: assistant }}]
      tokens: {{ input: message.usage.input_tokens }}
"#
    ))
    .expect("a valid binding")
}

#[test]
fn a_chain_deeper_than_the_guard_is_cut_at_the_guard() {
    // Given — a chain of 20 conversations, each the child of the one before
    let home = an_agent_home();
    let mut stem = "root".to_string();
    for _ in 0..20 {
        home.transcript(&format!("{stem}.jsonl")).assistant_message(
            &format!("m-{stem}"),
            "m",
            usage(1, 0),
        );
        stem = format!("{stem}.child");
    }

    // When
    let records = account(
        &a_binding_with_children("{dir}/{stem}.child.jsonl"),
        &home.session("root"),
    );

    // Then — the main conversation plus 16 levels of children
    assert_eq!(records.len(), 1 + 16);
}

#[test]
fn a_children_template_that_matches_its_own_parent_is_not_followed_again() {
    // Given
    let home = an_agent_home();
    home.transcript("root.jsonl")
        .assistant_message("m-root", "m", usage(1, 0));

    // When
    let records = account(
        &a_binding_with_children("{dir}/{stem}.jsonl"),
        &home.session("root"),
    );

    // Then
    let ids: Vec<&str> = records.iter().map(|r| r.id.as_str()).collect();
    assert_eq!(ids, vec!["root"]);
}
