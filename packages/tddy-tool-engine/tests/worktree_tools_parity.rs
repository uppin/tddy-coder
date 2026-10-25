//! `ListWorktrees` / `GetWorktree` show what the Worktrees screen shows, in its order, a page at a
//! time (`#start-parity` 3/9).
//!
//! The field list is read from the compiled-in proto descriptor; the order is pinned by a fixture
//! the web's `worktreeToolParity.cy.ts` runs the screen's own sort over.

use std::collections::BTreeSet;
use std::path::PathBuf;

use pretty_assertions::assert_eq;
use prost::Message;
use prost_types::FileDescriptorSet;
use tddy_tool_engine::project_home_worktree_tools::{
    page_worktrees, sort_worktrees_by_size, worktree_tool_defs, WorktreeEntry, GET_WORKTREE,
    LIST_WORKTREES, WORKTREE_PAGE_DEFAULT, WORKTREE_PAGE_MAX,
};

fn worktree_row_fields() -> BTreeSet<String> {
    let set = FileDescriptorSet::decode(tddy_service::SERVICE_DESCRIPTOR_BYTES)
        .expect("the compiled-in descriptor set decodes");
    set.file
        .iter()
        .filter(|file| file.package() == "worktree")
        .flat_map(|file| file.message_type.iter())
        .find(|message| message.name() == "WorktreeRow")
        .expect("worktree.WorktreeRow is in the descriptor set")
        .field
        .iter()
        .map(|field| field.name().to_string())
        .collect()
}

fn a_worktree(name: &str, disk_bytes: Option<u64>) -> WorktreeEntry {
    WorktreeEntry {
        path: format!("/repo/.worktrees/{name}"),
        branch_label: name.to_string(),
        disk_bytes,
        size_status: if disk_bytes.is_some() {
            "cached"
        } else {
            "none"
        }
        .to_string(),
        ..Default::default()
    }
}

fn names(entries: &[WorktreeEntry]) -> Vec<String> {
    entries
        .iter()
        .map(|e| e.path.rsplit('/').next().unwrap().to_string())
        .collect()
}

#[test]
fn a_listed_worktree_carries_every_field_the_screens_row_has() {
    // Given
    let row_fields = worktree_row_fields();

    // When
    let listed: BTreeSet<String> = serde_json::to_value(WorktreeEntry::default())
        .unwrap()
        .as_object()
        .unwrap()
        .keys()
        .cloned()
        .collect();

    // Then
    let missing: Vec<&String> = row_fields.difference(&listed).collect();
    assert!(
        missing.is_empty(),
        "WorktreeRow fields the tool does not list: {missing:?}"
    );
}

#[test]
fn worktrees_are_ordered_exactly_as_the_worktrees_screen_orders_them() {
    // Given the fixture the screen's own sort is checked against
    let fixture: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("../tddy-web/cypress/support/fixtures/worktree-sort-order.json"),
        )
        .expect("the sort fixture exists"),
    )
    .unwrap();
    let mut entries: Vec<WorktreeEntry> = fixture["input"]
        .as_array()
        .unwrap()
        .iter()
        .map(|row| a_worktree(row["name"].as_str().unwrap(), row["disk_bytes"].as_u64()))
        .collect();

    // When
    sort_worktrees_by_size(&mut entries);

    // Then
    let expected: Vec<String> = serde_json::from_value(fixture["sorted"].clone()).unwrap();
    assert_eq!(names(&entries), expected);
}

fn three_worktrees() -> Vec<WorktreeEntry> {
    vec![
        a_worktree("small", Some(5)),
        a_worktree("big", Some(300)),
        a_worktree("middle", Some(100)),
    ]
}

#[test]
fn a_page_holds_the_largest_first_with_the_total_and_a_cursor_to_the_rest() {
    // When
    let page = page_worktrees(three_worktrees(), "", Some(2)).expect("a first page");

    // Then
    assert_eq!(names(&page.worktrees), vec!["big", "middle"]);
    assert_eq!(page.total, 3);
    assert!(page.has_more);
    assert!(!page.next_cursor.is_empty());
}

#[test]
fn the_next_cursor_continues_without_repeats_or_gaps() {
    // Given
    let first = page_worktrees(three_worktrees(), "", Some(2)).unwrap();

    // When
    let second = page_worktrees(three_worktrees(), &first.next_cursor, Some(2)).unwrap();

    // Then
    assert_eq!(names(&second.worktrees), vec!["small"]);
    assert!(!second.has_more);
    assert_eq!(second.next_cursor, "");
}

#[test]
fn no_limit_is_the_default_page_and_a_limit_above_the_maximum_is_clamped_and_says_so() {
    // Given more worktrees than the largest page
    let many: Vec<WorktreeEntry> = (0..WORKTREE_PAGE_MAX + 10)
        .map(|i| a_worktree(&format!("w{i:04}"), Some(i as u64)))
        .collect();

    // When
    let unbounded_ask = page_worktrees(many.clone(), "", Some(10_000)).unwrap();
    let no_limit = page_worktrees(many, "", None).unwrap();

    // Then
    assert_eq!(unbounded_ask.worktrees.len(), WORKTREE_PAGE_MAX);
    assert_eq!(unbounded_ask.clamped_to, Some(WORKTREE_PAGE_MAX));
    assert_eq!(no_limit.worktrees.len(), WORKTREE_PAGE_DEFAULT);
    assert!(no_limit.has_more);
}

#[test]
fn a_cursor_the_tool_did_not_issue_is_refused() {
    // When
    let refused = page_worktrees(three_worktrees(), "not-a-cursor", Some(2));

    // Then
    assert!(refused.unwrap_err().contains("cursor"));
}

#[test]
fn sizing_progress_counts_the_worktrees_not_being_sized_of_all_of_them() {
    // Given one worktree being sized
    let mut entries = three_worktrees();
    entries[0].size_status = "calculating".to_string();

    // When
    let page = page_worktrees(entries, "", Some(1)).unwrap();

    // Then the progress is over every worktree, not the page
    assert_eq!((page.sizing.done, page.sizing.total), (2, 3));
}

#[test]
fn both_tools_are_defined_with_their_arguments_described() {
    // When
    let defs = worktree_tool_defs();

    // Then
    let schema_of = |name: &str| -> serde_json::Value {
        let def = defs
            .iter()
            .find(|d| d.name == name)
            .unwrap_or_else(|| panic!("{name} is defined"));
        serde_json::from_str(&def.input_schema_json).unwrap()
    };
    let list = schema_of(LIST_WORKTREES);
    for argument in ["cursor", "limit", "daemon_instance_id"] {
        assert!(
            !list["properties"][argument]["description"]
                .as_str()
                .unwrap_or_default()
                .is_empty(),
            "{LIST_WORKTREES}.{argument} is not described"
        );
    }
    let get = schema_of(GET_WORKTREE);
    assert_eq!(get["required"], serde_json::json!(["path"]));
}
