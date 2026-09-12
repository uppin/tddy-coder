use super::parse_agent_models_json;

#[test]
fn reads_the_models_and_default_from_the_tools_json() {
    // Given — the JSON contract emitted by `tddy-tools list-models`
    let stdout = r#"{"models":[{"id":"opus","label":"Claude Opus"},{"id":"sonnet","label":"Claude Sonnet"}],"default_model":"opus"}"#;

    // When
    let resp = parse_agent_models_json(stdout).expect("well-formed catalog should parse");

    // Then
    assert_eq!(resp.default_model, "opus");
    let ids: Vec<&str> = resp.models.iter().map(|m| m.id.as_str()).collect();
    assert_eq!(ids, vec!["opus", "sonnet"]);
    assert_eq!(resp.models[0].label, "Claude Opus");
}

#[test]
fn errors_on_malformed_probe_output() {
    // When / Then — garbage is a hard error, never an empty catalog
    assert!(parse_agent_models_json("not json at all").is_err());
}
