//! The index's answers in the web's vocabulary, and the web's positions in the index's — the one
//! place each `code_index` message is mapped to its `code_navigation` counterpart, shared by the
//! unary methods and the code session.

use tddy_index_daemon::proto::code_index as index;
use tddy_service::proto::code_navigation::{
    CodeIndexPhase, CodeIndexProgress, CodeIndexState, CodeLocation, DefinitionResponse,
    HoverResponse, ReferencesResponse, SourcePosition, SourceRange,
};

/// The index's state of a root in the web's vocabulary.
pub(crate) fn web_state(state: index::IndexState) -> CodeIndexState {
    match state {
        index::IndexState::Unspecified => CodeIndexState::Unspecified,
        index::IndexState::Idle => CodeIndexState::Idle,
        index::IndexState::Loading => CodeIndexState::Loading,
        index::IndexState::Ready => CodeIndexState::Ready,
        index::IndexState::Failed => CodeIndexState::Failed,
    }
}

/// A root's progress, as the index reports it, in the web's vocabulary — field for field.
pub(crate) fn web_progress(update: index::IndexProgress) -> CodeIndexProgress {
    CodeIndexProgress {
        state: web_state(update.state()) as i32,
        line: update.line,
        phase: update.phase,
        percentage: update.percentage,
        furthest: update.furthest,
        ready: update.ready,
        error: update.error,
        phases: update
            .phases
            .into_iter()
            .map(|phase| CodeIndexPhase {
                title: phase.title,
                message: phase.message,
                percentage: phase.percentage,
            })
            .collect(),
    }
}

/// The index's definition answer in the web's vocabulary.
pub(crate) fn web_definition(answer: index::DefinitionResponse) -> DefinitionResponse {
    DefinitionResponse {
        state: web_state(answer.state()) as i32,
        locations: web_locations(answer.locations),
    }
}

/// The index's references answer in the web's vocabulary.
pub(crate) fn web_references(answer: index::ReferencesResponse) -> ReferencesResponse {
    ReferencesResponse {
        state: web_state(answer.state()) as i32,
        locations: web_locations(answer.locations),
    }
}

/// The index's hover answer in the web's vocabulary.
pub(crate) fn web_hover(answer: index::HoverResponse) -> HoverResponse {
    HoverResponse {
        state: web_state(answer.state()) as i32,
        markdown: answer.markdown,
    }
}

/// The web's position in the index's vocabulary.
pub(crate) fn index_position(position: SourcePosition) -> index::SourcePosition {
    index::SourcePosition {
        line: position.line,
        column: position.column,
    }
}

fn web_position(position: index::SourcePosition) -> SourcePosition {
    SourcePosition {
        line: position.line,
        column: position.column,
    }
}

/// Every one of the index's locations in the web's vocabulary.
pub(crate) fn web_locations(locations: Vec<index::CodeLocation>) -> Vec<CodeLocation> {
    locations.into_iter().map(web_location).collect()
}

/// The index's location in the web's vocabulary: `file` becomes `rel_path` and `outside_root`
/// becomes `outside_worktree`; the path is already relative to the root when inside it and
/// absolute otherwise, which is what `rel_path` is documented to hold.
pub(crate) fn web_location(location: index::CodeLocation) -> CodeLocation {
    CodeLocation {
        rel_path: location.file,
        range: location.range.map(|range| SourceRange {
            start: range.start.map(web_position),
            end: range.end.map(web_position),
        }),
        outside_worktree: location.outside_root,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_location_inside_the_root_maps_field_for_field() {
        // Given an index location inside the workspace root
        let inside = index::CodeLocation {
            file: "src/lib.rs".to_string(),
            range: Some(index::SourceRange {
                start: Some(index::SourcePosition { line: 3, column: 1 }),
                end: Some(index::SourcePosition { line: 3, column: 9 }),
            }),
            outside_root: false,
        };

        // When it is mapped for the web
        let mapped = web_location(inside);

        // Then it keeps its path and range and is not marked outside the worktree
        assert_eq!(
            mapped,
            CodeLocation {
                rel_path: "src/lib.rs".to_string(),
                range: Some(SourceRange {
                    start: Some(SourcePosition { line: 3, column: 1 }),
                    end: Some(SourcePosition { line: 3, column: 9 }),
                }),
                outside_worktree: false,
            }
        );
    }

    #[test]
    fn a_location_outside_the_root_is_marked_outside_the_worktree() {
        // Given an index location in a dependency, which the index names by absolute path
        let dependency = index::CodeLocation {
            file: "/home/u/.cargo/registry/src/serde/lib.rs".to_string(),
            range: None,
            outside_root: true,
        };

        // When it is mapped for the web
        let mapped = web_location(dependency);

        // Then the pane is told not to offer opening it, and still gets the absolute path
        assert!(mapped.outside_worktree);
        assert_eq!(mapped.rel_path, "/home/u/.cargo/registry/src/serde/lib.rs");
    }
}
