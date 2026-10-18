//! Asking the index one code-session query: the pane's query in the index's terms, and the index's
//! answer in the pane's — or [`Reply::Loading`] when it is not final yet.

use tddy_index_daemon::proto::code_index as index;
use tddy_index_daemon::proto::tonic_code_index::code_index_service_client::CodeIndexServiceClient;
use tddy_service::proto::code_navigation::code_query_answer::Result as Answered;
use tddy_service::proto::code_navigation::{
    CodeQuery, CodeQueryKind, Definability, DefinitionResponse, HoverResponse, ProbeAnswer,
    ReferencesResponse,
};
use tddy_worktree_service::worktree_files::validate_rel_path_shape;
use tonic::transport::Channel;

use crate::code_navigation::{index_position, web_location, web_state};

/// What a query asks the index.
#[derive(Clone, Copy)]
pub(super) enum Kind {
    Hover,
    Definition,
    References,
    Probe,
}

/// A query the pane asked, in the index's terms.
#[derive(Clone)]
pub(super) struct Query {
    pub(super) request_id: u64,
    kind: Kind,
    file: String,
    position: Option<index::SourcePosition>,
    /// Whether the pane has been told this query is held.
    pub(super) held: bool,
}

impl Query {
    /// The pane's query, or why it cannot be asked.
    pub(super) fn parse(query: CodeQuery) -> Result<Self, String> {
        let kind = match CodeQueryKind::try_from(query.kind) {
            Ok(CodeQueryKind::Hover) => Kind::Hover,
            Ok(CodeQueryKind::Definition) => Kind::Definition,
            Ok(CodeQueryKind::References) => Kind::References,
            Ok(CodeQueryKind::Probe) => Kind::Probe,
            Ok(CodeQueryKind::Unspecified) | Err(_) => {
                return Err(format!(
                    "query kind {} is not one this session answers",
                    query.kind
                ))
            }
        };
        let file = validate_rel_path_shape(&query.rel_path).map_err(|s| s.message().to_string())?;
        Ok(Self {
            request_id: query.request_id,
            kind,
            file,
            position: query.position.map(index_position),
            held: false,
        })
    }
}

/// What the index made of a query.
pub(super) enum Reply {
    Answer(Answered),
    /// The index is loading, so its empty answer is not final: the query waits.
    Loading,
}

/// `query` asked of the index, rooted at `workspace_root`.
pub(super) async fn ask_index(
    mut client: CodeIndexServiceClient<Channel>,
    workspace_root: String,
    query: &Query,
) -> Result<Reply, tonic::Status> {
    let file = query.file.clone();
    let position = query.position;
    let loading = |state: index::IndexState| state == index::IndexState::Loading;
    Ok(match query.kind {
        Kind::Hover => {
            let answer = client
                .hover(index::HoverRequest {
                    workspace_root,
                    file,
                    position,
                })
                .await?
                .into_inner();
            if loading(answer.state()) {
                Reply::Loading
            } else {
                Reply::Answer(Answered::Hover(HoverResponse {
                    state: web_state(answer.state()) as i32,
                    markdown: answer.markdown,
                }))
            }
        }
        Kind::Definition | Kind::Probe => {
            let answer = client
                .definition(index::DefinitionRequest {
                    workspace_root,
                    file,
                    position,
                })
                .await?
                .into_inner();
            match query.kind {
                Kind::Probe => Reply::Answer(Answered::Probe(probe_answer(answer))),
                _ if loading(answer.state()) => Reply::Loading,
                _ => Reply::Answer(Answered::Definition(DefinitionResponse {
                    state: web_state(answer.state()) as i32,
                    locations: answer.locations.into_iter().map(web_location).collect(),
                })),
            }
        }
        Kind::References => {
            let answer = client
                .references(index::ReferencesRequest {
                    workspace_root,
                    file,
                    position,
                })
                .await?
                .into_inner();
            if loading(answer.state()) {
                Reply::Loading
            } else {
                Reply::Answer(Answered::References(ReferencesResponse {
                    state: web_state(answer.state()) as i32,
                    locations: answer.locations.into_iter().map(web_location).collect(),
                }))
            }
        }
    })
}

/// Whether a definition answer gives the pane somewhere to go: `UNKNOWN` while the index loads,
/// otherwise `DEFINABLE` with where, or `NONE`.
fn probe_answer(answer: index::DefinitionResponse) -> ProbeAnswer {
    if answer.state() == index::IndexState::Loading {
        return ProbeAnswer {
            definability: Definability::Unknown as i32,
            locations: Vec::new(),
        };
    }
    let definability = if answer.locations.is_empty() {
        Definability::None
    } else {
        Definability::Definable
    };
    ProbeAnswer {
        definability: definability as i32,
        locations: answer.locations.into_iter().map(web_location).collect(),
    }
}
