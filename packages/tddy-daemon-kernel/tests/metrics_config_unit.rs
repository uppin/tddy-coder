//! `metrics:` in daemon.yaml: a private listener of its own, never the web port.

use tddy_daemon_kernel::config::{check_metrics_listen, DaemonConfig};

fn parsed(yaml: &str) -> DaemonConfig {
    serde_yaml::from_str(yaml).expect("daemon config parses")
}

#[test]
fn a_metrics_section_names_the_private_listener() {
    let config = parsed("metrics:\n  listen: \"127.0.0.1:9464\"\n");
    assert_eq!(
        config.metrics.unwrap().listen,
        "127.0.0.1:9464".parse().unwrap()
    );
}

#[test]
fn no_metrics_section_means_no_metrics_listener() {
    assert!(parsed("listen:\n  web_port: 8899\n").metrics.is_none());
}

#[test]
fn metrics_on_the_web_port_is_refused_naming_both_settings() {
    let config = parsed("listen:\n  web_port: 8899\nmetrics:\n  listen: \"0.0.0.0:8899\"\n");

    let refusal = check_metrics_listen(&config.listen, config.metrics.as_ref()).unwrap_err();

    assert!(refusal.contains("metrics.listen"), "{refusal}");
    assert!(refusal.contains("listen.web_port"), "{refusal}");
}

#[test]
fn metrics_on_its_own_port_is_accepted() {
    let config = parsed("listen:\n  web_port: 8899\nmetrics:\n  listen: \"127.0.0.1:9464\"\n");
    assert_eq!(
        check_metrics_listen(&config.listen, config.metrics.as_ref()),
        Ok(())
    );
}
