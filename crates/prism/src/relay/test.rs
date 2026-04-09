use super::*;

#[test]
fn relay_mode_standard_copy_str() {
    assert_eq!(RelayMode::StandardCopy.as_str(), "standard-copy");
    assert_eq!(RelayMode::StandardCopy.to_string(), "standard-copy");
}

#[test]
fn relay_stats_default() {
    let stats = RelayStats::default();
    assert_eq!(stats.upload_bytes, 0);
    assert_eq!(stats.download_bytes, 0);
    assert!(stats.mode.is_none());
}
