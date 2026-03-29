//! Red: YAML authority parsing must return fixture ids once `load_authority_ids_from_yaml` is implemented.

#[test]
fn remote_config_yaml_parses_authority_ids() {
    let yaml = r#"
authorities:
  - id: "unit-fixture-alpha"
    connect_base_url: "http://127.0.0.1:9"
  - id: "unit-fixture-beta"
    connect_base_url: "http://127.0.0.1:9"
"#;
    let ids = tddy_remote::load_authority_ids_from_yaml(yaml).expect("parse authorities");
    assert_eq!(
        ids,
        vec![
            "unit-fixture-alpha".to_string(),
            "unit-fixture-beta".to_string(),
        ]
    );
}
