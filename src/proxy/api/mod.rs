#[cfg(feature = "http-api")]
mod client;
mod service;

pub use service::ApiService;
