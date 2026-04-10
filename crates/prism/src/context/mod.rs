use std::sync::Arc;

use arc_swap::ArcSwap;

use crate::config::Config;
use crate::hooks::PrismHooks;
use crate::players::ConnectionRegistry;
use crate::stats::ConnectionTotals;

#[derive(Default)]
pub struct PrismRuntime {
    pub connections: ConnectionRegistry,
    pub totals: ConnectionTotals,
}

pub struct PrismContext<H: PrismHooks> {
    runtime: Arc<PrismRuntime>,
    config: Arc<ArcSwap<Config>>,
    hooks: Arc<H>,
}

impl<H: PrismHooks> PrismContext<H> {
    pub fn new(config: Config, hooks: H) -> Self {
        let runtime = Arc::new(PrismRuntime::default());
        Self {
            runtime,
            config: Arc::new(ArcSwap::from(Arc::new(config))),
            hooks: Arc::new(hooks),
        }
    }

    pub fn runtime(&self) -> &Arc<PrismRuntime> {
        &self.runtime
    }

    pub fn config(&self) -> arc_swap::Guard<Arc<Config>> {
        self.config.load()
    }

    pub fn hooks(&self) -> &Arc<H> {
        &self.hooks
    }

    pub fn update_config(&self, config: Config) {
        self.config.store(Arc::new(config));
    }
}

impl<H: PrismHooks> Clone for PrismContext<H> {
    fn clone(&self) -> Self {
        Self {
            runtime: self.runtime.clone(),
            config: self.config.clone(),
            hooks: self.hooks.clone(),
        }
    }
}
