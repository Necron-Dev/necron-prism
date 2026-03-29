use super::PlayerRegistry;

#[test]
fn registry_tracks_active_sessions() {
    let registry = PlayerRegistry::default();

    assert_eq!(registry.register_connection(1), 1);
    assert_eq!(registry.active_count(), 1);
    assert_eq!(registry.remove_connection(1), 0);
    assert_eq!(registry.active_count(), 0);
}
