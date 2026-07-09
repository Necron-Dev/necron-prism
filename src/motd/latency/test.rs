use prism::config::{LatencyNeeds, MotdConfig, MotdMode};

#[test]
fn latency_needs_skip_probe_without_latency_placeholders() {
    let config = MotdConfig {
        mode: MotdMode::Local,
        local_json: "{\"description\":{\"text\":\"plain motd\"}}".to_owned(),
        ..MotdConfig::default()
    };

    let needs = LatencyNeeds::for_config(&config);

    assert!(!needs.client);
    assert!(!needs.upstream);
}

#[test]
fn latency_needs_total_placeholder_uses_client_and_upstream() {
    let config = MotdConfig {
        mode: MotdMode::Local,
        local_json: "{\"description\":{\"text\":\"{total_latency}\"}}".to_owned(),
        ..MotdConfig::default()
    };

    let needs = LatencyNeeds::for_config(&config);

    assert!(needs.client);
    assert!(needs.upstream);
}

#[test]
fn latency_needs_ignore_upstream_mode_template() {
    let config = MotdConfig {
        mode: MotdMode::Upstream,
        local_json: "{\"description\":{\"text\":\"{total_latency}\"}}".to_owned(),
        ..MotdConfig::default()
    };

    let needs = LatencyNeeds::for_config(&config);

    assert!(!needs.client);
    assert!(!needs.upstream);
}

#[test]
fn latency_needs_individual_placeholders() {
    let config = MotdConfig {
        mode: MotdMode::Local,
        local_json: "{\"description\":{\"text\":\"{client_latency} {upstream_latency}\"}}"
            .to_owned(),
        ..MotdConfig::default()
    };

    let needs = LatencyNeeds::for_config(&config);

    assert!(needs.client);
    assert!(needs.upstream);
}
