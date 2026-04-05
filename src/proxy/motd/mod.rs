mod context;
mod legacy;
mod rewrite;
mod service;
mod upstream;

pub use legacy::serve_legacy_ping;
pub use service::MotdService;
