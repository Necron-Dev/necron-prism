use super::players::PlayerRegistry;

pub fn render(value: &str, players: &PlayerRegistry) -> String {
    value.replace(
        "%ONLINE_PLAYER%",
        &players.current_online_count().to_string(),
    )
}
