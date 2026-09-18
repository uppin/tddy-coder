use tddy_core::ParseError;

/// Parsed output from the bugfix `analyze` goal (`tddy-tools submit --goal analyze`).
#[derive(Debug, Clone)]
pub struct AnalyzeOutput {
    pub branch_suggestion: String,
    pub worktree_suggestion: String,
    pub name: Option<String>,
    pub summary: Option<String>,
    /// Code-discovery knowledge to persist as `artifacts/exploration.md` (analyze is bugfix's discovery step).
    pub exploration: Option<String>,
}

#[derive(serde::Deserialize)]
struct StructuredAnalyze {
    pub(crate) goal: Option<String>,
    pub(crate) branch_suggestion: Option<String>,
    pub(crate) worktree_suggestion: Option<String>,
    pub(crate) name: Option<String>,
    pub(crate) summary: Option<String>,
    pub(crate) exploration: Option<String>,
}

/// Parse LLM analyze response. JSON must come from tddy-tools submit.
pub fn parse_analyze_response(s: &str) -> Result<AnalyzeOutput, ParseError> {
    let s = s.trim();
    let parsed: StructuredAnalyze = serde_json::from_str(s)
        .map_err(|e| ParseError::Malformed(format!("invalid JSON: {}", e)))?;
    if parsed.goal.as_deref() != Some("analyze") {
        return Err(ParseError::Malformed(format!(
            "goal is not analyze, got: {:?}",
            parsed.goal
        )));
    }
    let branch_suggestion = parsed
        .branch_suggestion
        .filter(|x| !x.is_empty())
        .ok_or_else(|| ParseError::Malformed("branch_suggestion missing or empty".into()))?;
    let worktree_suggestion = parsed
        .worktree_suggestion
        .filter(|x| !x.is_empty())
        .ok_or_else(|| ParseError::Malformed("worktree_suggestion missing or empty".into()))?;
    Ok(AnalyzeOutput {
        branch_suggestion,
        worktree_suggestion,
        name: parsed.name.filter(|x| !x.is_empty()),
        summary: parsed.summary.filter(|x| !x.is_empty()),
        exploration: parsed.exploration.filter(|x| !x.trim().is_empty()),
    })
}
