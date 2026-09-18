use tddy_core::ParseError;

/// Parsed green goal output.
#[derive(Debug, Clone)]
pub struct GreenOutput {
    pub summary: String,
    pub tests: Vec<GreenTestResult>,
    pub implementations: Vec<ImplementationInfo>,
    pub test_command: Option<String>,
    pub prerequisite_actions: Option<String>,
    pub run_single_or_selected_tests: Option<String>,
    /// Demo results when demo-plan.md was present and green completed.
    pub demo_results: Option<DemoResults>,
}

/// Demo execution results from green goal.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct DemoResults {
    pub summary: String,
    pub steps_completed: u32,
}

/// Parsed output from the standalone demo goal.
#[derive(Debug, Clone)]
pub struct DemoOutput {
    pub summary: String,
    pub demo_type: String,
    pub steps_completed: u32,
    pub verification: String,
    /// Shareable URL produced by the demo (e.g. `http://localhost:8080` for PortForward,
    /// or a LiveKit viewer URL for ScreenShare). `None` if the demo did not produce a link.
    pub share_url: Option<String>,
}

/// Info about a single test result from the green goal.
#[derive(Debug, Clone)]
pub struct GreenTestResult {
    pub name: String,
    pub file: String,
    pub line: Option<u32>,
    pub status: String,
    pub reason: Option<String>,
}

/// Info about an implementation (method, struct, etc.) from the green goal.
#[derive(Debug, Clone)]
pub struct ImplementationInfo {
    pub name: String,
    pub file: String,
    pub line: Option<u32>,
    pub kind: String,
}

#[derive(serde::Deserialize)]
struct StructuredGreen {
    pub(crate) goal: Option<String>,
    pub(crate) summary: Option<String>,
    pub(crate) tests: Option<Vec<GreenTestResultDe>>,
    pub(crate) implementations: Option<Vec<ImplementationInfoDe>>,
    pub(crate) test_command: Option<String>,
    pub(crate) prerequisite_actions: Option<String>,
    pub(crate) run_single_or_selected_tests: Option<String>,
    #[serde(default)]
    pub(crate) demo_results: Option<DemoResultsDe>,
}

#[derive(serde::Deserialize)]
struct DemoResultsDe {
    pub(crate) summary: String,
    pub(crate) steps_completed: u32,
}

#[derive(serde::Deserialize)]
struct GreenTestResultDe {
    pub(crate) name: String,
    pub(crate) file: String,
    pub(crate) line: Option<u32>,
    pub(crate) status: String,
    #[serde(default)]
    pub(crate) reason: Option<String>,
}

#[derive(serde::Deserialize)]
struct ImplementationInfoDe {
    pub(crate) name: String,
    pub(crate) file: String,
    pub(crate) line: Option<u32>,
    pub(crate) kind: String,
}

/// Parse LLM green goal response. JSON must come from tddy-tools submit.
pub fn parse_green_response(s: &str) -> Result<GreenOutput, ParseError> {
    let s = s.trim();
    let parsed: StructuredGreen = serde_json::from_str(s)
        .map_err(|e| ParseError::Malformed(format!("invalid JSON: {}", e)))?;
    if parsed.goal.as_deref() != Some("green") {
        return Err(ParseError::Malformed("goal is not green".into()));
    }
    let summary = parsed
        .summary
        .filter(|x| !x.is_empty())
        .ok_or_else(|| ParseError::Malformed("summary missing or empty".into()))?;
    let tests = parsed
        .tests
        .unwrap_or_default()
        .into_iter()
        .map(|t| GreenTestResult {
            name: t.name,
            file: t.file,
            line: t.line,
            status: t.status,
            reason: t.reason,
        })
        .collect();
    let implementations = parsed
        .implementations
        .unwrap_or_default()
        .into_iter()
        .map(|i| ImplementationInfo {
            name: i.name,
            file: i.file,
            line: i.line,
            kind: i.kind,
        })
        .collect();
    let demo_results = parsed.demo_results.map(|d| DemoResults {
        summary: d.summary,
        steps_completed: d.steps_completed,
    });
    Ok(GreenOutput {
        summary,
        tests,
        implementations,
        test_command: parsed.test_command.filter(|x| !x.is_empty()),
        prerequisite_actions: parsed.prerequisite_actions.filter(|x| !x.is_empty()),
        run_single_or_selected_tests: parsed
            .run_single_or_selected_tests
            .filter(|x| !x.is_empty()),
        demo_results,
    })
}

impl GreenOutput {
    /// Render updated progress.md with [x] for passing, [!] for failing.
    pub fn to_updated_progress_markdown(&self) -> String {
        let mut out = String::from("# Progress\n\n");
        out.push_str("Unfilled milestones. Mark each as done [x], skipped, or failed.\n\n");
        out.push_str("## Failed Tests\n\n");
        for t in &self.tests {
            let loc = t
                .line
                .map(|l| format!("{}:{}", t.file, l))
                .unwrap_or_else(|| t.file.clone());
            let marker = if t.status == "passing" { "[x]" } else { "[!]" };
            let reason = t
                .reason
                .as_deref()
                .map(|r| format!(" — {}", r))
                .unwrap_or_default();
            out.push_str(&format!("- {} {} ({}){}\n", marker, t.name, loc, reason));
        }
        out.push_str("\n## Skeletons\n\n");
        for i in &self.implementations {
            let loc = i
                .line
                .map(|l| format!("{}:{}", i.file, l))
                .unwrap_or_else(|| i.file.clone());
            out.push_str(&format!("- [x] {} ({}) — {}\n", i.name, loc, i.kind));
        }
        out
    }

    /// Update acceptance-tests.md content: replace "failing" with "passing" for passing tests.
    pub fn update_acceptance_tests_content(&self, content: &str) -> String {
        let passing: std::collections::HashSet<&str> = self
            .tests
            .iter()
            .filter(|t| t.status == "passing")
            .map(|t| t.name.as_str())
            .collect();
        if passing.is_empty() {
            return content.to_string();
        }
        let mut out = String::new();
        let sections: Vec<&str> = content.split("\n### ").collect();
        for (i, section) in sections.iter().enumerate() {
            if i == 0 {
                out.push_str(section);
                if sections.len() > 1 {
                    out.push_str("\n### ");
                }
                continue;
            }
            let (name, rest) = section.split_once('\n').unwrap_or((section, ""));
            let test_name = name.trim();
            let updated_rest = if passing.contains(test_name) {
                rest.replace("- **Status**: failing", "- **Status**: passing")
            } else {
                rest.to_string()
            };
            out.push_str(test_name);
            out.push('\n');
            out.push_str(&updated_rest);
            if i < sections.len() - 1 {
                out.push_str("\n### ");
            }
        }
        out
    }

    /// Returns true if all tests are passing.
    pub fn all_tests_passing(&self) -> bool {
        self.tests.iter().all(|t| t.status == "passing")
    }
}
