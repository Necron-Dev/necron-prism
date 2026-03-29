use anyhow::Result;
use std::sync::Arc;

use super::super::api::ApiService;
use super::super::config::Config;
use super::super::motd::MotdService;
use super::super::players::PlayerRegistry;
use super::super::stats::{ConnectionStats, ConnectionTotals};
use super::super::traffic::TrafficReporter;

#[derive(Clone)]
pub struct AppState {
    pub config: Arc<Config>,
    pub api: Arc<ApiService>,
    pub motd: Arc<MotdService>,
    pub traffic_reporter: Arc<TrafficReporter>,
    pub players: PlayerRegistry,
    pub connection_stats: ConnectionStats,
    pub connection_totals: ConnectionTotals,
}

impl AppState {
    pub fn new(config: Config) -> Result<Self> {
        let config = Arc::new(config);
        let api = Arc::new(ApiService::new(&config.api)?);
        let motd = Arc::new(MotdService::default());
        let traffic_reporter = Arc::new(TrafficReporter::new(Arc::clone(&api), &config.api));

        Ok(Self {
            config,
            api,
            motd,
            traffic_reporter,
            players: PlayerRegistry::default(),
            connection_stats: ConnectionStats::default(),
            connection_totals: ConnectionTotals::default(),
        })
    }
}
