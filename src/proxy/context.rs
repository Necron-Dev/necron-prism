use arc_swap::ArcSwap;
use std::sync::atomic::AtomicU64;
use std::sync::Arc;
use tracing_subscriber::EnvFilter;

use crate::proxy::config::{Config, ConfigLoader};
use crate::proxy::logging::LogHandle;
use crate::proxy::players::PlayerRegistry;
use crate::proxy::stats::{ConnectionStats, ConnectionTotals};
use crate::proxy::traffic::{spawn_stats_logger, StatsLoggerHandle, TrafficReporter};

use super::api::ApiService;
use super::motd::MotdService;

#[derive(Default)]
pub struct Core {
    pub players: PlayerRegistry,
    pub stats: ConnectionStats,
    pub totals: ConnectionTotals,
    counter: Arc<AtomicU64>,
}

pub struct Services {
    pub api: Arc<ApiService>,
    pub motd: Arc<MotdService>,
    pub traffic: TrafficReporter,
    pub logger: Option<StatsLoggerHandle>,
}

#[derive(Clone)]
pub struct Context {
    pub core: Arc<Core>,
    config: Arc<ArcSwap<Config>>,
    services: Arc<ArcSwap<Services>>,
}

impl Context {
    pub fn new(config: Config) -> anyhow::Result<Self> {
        let core = Arc::new(Core::default());
        Self::build(config, core)
    }

    fn build(config: Config, core: Arc<Core>) -> anyhow::Result<Self> {
        let config = Arc::new(ArcSwap::from(Arc::new(config)));
        let services = Self::create_services(&config, &core)?;

        Ok(Self {
            core,
            config,
            services: Arc::new(ArcSwap::from(Arc::new(services))),
        })
    }

    fn create_services(
        config: &Arc<ArcSwap<Config>>,
        core: &Arc<Core>,
    ) -> anyhow::Result<Services> {
        let loaded = config.load();
        let api = Arc::new(ApiService::new(&loaded.api, core.counter.clone())?);
        let motd = Arc::new(MotdService::new());
        let traffic = TrafficReporter::new(api.clone(), &loaded.api);

        let logger = loaded.stats_log_interval().map(|interval| {
            spawn_stats_logger(
                core.stats.clone(),
                core.totals.clone(),
                core.players.clone(),
                traffic.clone(),
                interval,
            )
        });

        Ok(Services {
            api,
            motd,
            traffic,
            logger,
        })
    }

    pub fn config(&self) -> arc_swap::Guard<Arc<Config>> {
        self.config.load()
    }

    pub fn services(&self) -> arc_swap::Guard<Arc<Services>> {
        self.services.load()
    }

    pub fn reload(&self, log_handle: &LogHandle) -> anyhow::Result<()> {
        let new_config = ConfigLoader::load_default()?;
        new_config.validate()?;

        let filter = EnvFilter::new(new_config.logging.level.as_filter_directive());
        log_handle.modify(|f| *f = filter)?;

        let old = self.services.load();
        if let Some(h) = &old.logger {
            h.shutdown();
        }
        old.traffic.shutdown();

        self.config.store(Arc::new(new_config));
        let new_services = Self::create_services(&self.config, &self.core)?;
        self.services.store(Arc::new(new_services));

        Ok(())
    }
}
