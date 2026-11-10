//! The conversation tree: children located from each conversation's files, to any depth.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use crate::jsonl::fold_files;
use crate::template::TemplateError;
use crate::token_accounting::ConversationRecord;
use crate::{ChildrenSpec, FieldPath, JsonlSource, PathTemplate, UsageContext};

/// How many levels of children below the main conversation are followed.
const MAX_DEPTH: usize = 16;

/// The variables a children template binds: the parent or child file's directory and stem.
pub(crate) const FILE_VARIABLES: &[&str] = &["dir", "stem"];

/// The main conversation's variables.
pub(crate) const MAIN_VARIABLES: &[&str] =
    &["agent_home", "session_dir", "conversation_id", "worktree"];

/// The rows `source` yields for the session `ctx` describes: the main conversation (every file
/// matching `main.path` merged into one row), then its children depth-first, siblings by id.
/// `None` when the session has no conversation id to look for.
pub(crate) fn conversation_rows(
    source: &JsonlSource,
    ctx: &UsageContext,
) -> Option<Vec<ConversationRecord>> {
    let id = ctx.conversation_id.clone()?;
    let files = main_files(source, ctx);
    let main = row(&ctx.agent_label, &id, None, &files, source, ctx);
    let mut rows = vec![main];

    let mut visited: HashSet<PathBuf> = files.iter().cloned().collect();
    if let Some(children) = &source.children {
        let mut found = Vec::new();
        for file in &files {
            found.extend(children_of(
                file,
                children,
                &id,
                1,
                &mut visited,
                source,
                ctx,
            ));
        }
        found.sort_by(|a, b| a.0.id.cmp(&b.0.id));
        for (child, descendants) in found {
            rows.push(child);
            rows.extend(descendants);
        }
    }
    Some(rows)
}

/// The files `main.path` matches, in path order. A template that cannot be rendered — a variable
/// with no value for this session — matches none.
fn main_files(source: &JsonlSource, ctx: &UsageContext) -> Vec<PathBuf> {
    let lookup = |name: &str| -> Option<String> {
        let value = match name {
            "agent_home" => ctx.agent_home.as_ref().map(|p| p.display().to_string()),
            "session_dir" => Some(ctx.session_dir.display().to_string()),
            "conversation_id" => ctx.conversation_id.clone(),
            "worktree" => ctx.worktree.as_ref().map(|p| p.display().to_string()),
            _ => None,
        };
        value.map(|v| glob::Pattern::escape(&v))
    };
    match source.main.path.render(MAIN_VARIABLES, &lookup) {
        Ok(pattern) => glob_files(&pattern),
        Err(TemplateError::Unset { .. }) => Vec::new(),
        Err(e) => {
            log::warn!("usage: {e}");
            Vec::new()
        }
    }
}

/// The child conversations of `parent_file`, each with its own descendants, in sibling order.
fn children_of(
    parent_file: &Path,
    children: &ChildrenSpec,
    parent_id: &str,
    depth: usize,
    visited: &mut HashSet<PathBuf>,
    source: &JsonlSource,
    ctx: &UsageContext,
) -> Vec<(ConversationRecord, Vec<ConversationRecord>)> {
    if depth > MAX_DEPTH {
        return Vec::new();
    }
    let Some(pattern) = render_over_file(&children.path, parent_file, true) else {
        return Vec::new();
    };
    let mut found = Vec::new();
    for file in glob_files(&pattern) {
        if !visited.insert(file.clone()) {
            continue;
        }
        let Some(id) = render_over_file(&children.id, &file, false) else {
            continue;
        };
        let name = child_name(children, &file, &id);
        let record = row(
            &name,
            &id,
            Some(parent_id),
            std::slice::from_ref(&file),
            source,
            ctx,
        );
        let mut descendants = Vec::new();
        let mut grandchildren = children_of(&file, children, &id, depth + 1, visited, source, ctx);
        grandchildren.sort_by(|a, b| a.0.id.cmp(&b.0.id));
        for (grandchild, rest) in grandchildren {
            descendants.push(grandchild);
            descendants.extend(rest);
        }
        found.push((record, descendants));
    }
    found.sort_by(|a, b| a.0.id.cmp(&b.0.id));
    found
}

/// The child's display name: the sidecar's field, else the spec's default; its id when the spec
/// declares no name.
fn child_name(children: &ChildrenSpec, file: &Path, id: &str) -> String {
    let Some(name) = &children.name else {
        return id.to_string();
    };
    render_over_file(&name.sidecar, file, false)
        .and_then(|path| std::fs::read_to_string(path).ok())
        .and_then(|text| serde_json::from_str::<serde_json::Value>(&text).ok())
        .and_then(|json| field_string(&name.field, &json))
        .unwrap_or_else(|| name.default.clone())
}

fn field_string(field: &FieldPath, json: &serde_json::Value) -> Option<String> {
    field.lookup(json)?.as_str().map(str::to_string)
}

/// `template` over `file`'s `{dir}` and `{stem}`; glob metacharacters in the values are escaped
/// when the result is a glob pattern.
fn render_over_file(template: &PathTemplate, file: &Path, as_glob: bool) -> Option<String> {
    let dir = file.parent()?.display().to_string();
    let stem = file.file_stem()?.to_string_lossy().into_owned();
    let lookup = |name: &str| -> Option<String> {
        let value = match name {
            "dir" => dir.clone(),
            "stem" => stem.clone(),
            _ => return None,
        };
        Some(if as_glob {
            glob::Pattern::escape(&value)
        } else {
            value
        })
    };
    match template.render(FILE_VARIABLES, &lookup) {
        Ok(text) => Some(text),
        Err(e) => {
            log::warn!("usage: {e}");
            None
        }
    }
}

fn glob_files(pattern: &str) -> Vec<PathBuf> {
    match glob::glob(pattern) {
        Ok(paths) => {
            let mut files: Vec<PathBuf> = paths
                .filter_map(Result::ok)
                .filter(|p| p.is_file())
                .collect();
            files.sort();
            files
        }
        Err(e) => {
            log::warn!("usage: bad path pattern '{pattern}': {e}");
            Vec::new()
        }
    }
}

/// One conversation's row, folded from `files` (none: a zero row on the fallback model).
fn row(
    agent: &str,
    id: &str,
    parent_id: Option<&str>,
    files: &[PathBuf],
    source: &JsonlSource,
    ctx: &UsageContext,
) -> ConversationRecord {
    let folded = fold_files(files, &source.record);
    ConversationRecord {
        agent: agent.to_string(),
        id: id.to_string(),
        model: folded.model.unwrap_or_else(|| ctx.fallback_model.clone()),
        input_tokens: folded.input,
        output_tokens: folded.output,
        total_tokens: folded.input
            + folded.cache_read
            + folded.cache_write
            + folded.output
            + folded.reasoning,
        turns: folded.turns,
        cache_read_tokens: folded.cache_read,
        cache_write_tokens: folded.cache_write,
        reasoning_tokens: folded.reasoning,
        cost_usd: folded.cost,
        parent_id: parent_id.map(str::to_string),
    }
}
