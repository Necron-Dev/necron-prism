mod cache;
mod context;
mod legacy;
mod service;
mod upstream;

pub use legacy::serve_legacy_ping;
pub use service::MotdService;
