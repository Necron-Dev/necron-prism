#[cfg(feature = "http-api")]
mod client;
mod service;
mod types;

pub use service::ApiService;
