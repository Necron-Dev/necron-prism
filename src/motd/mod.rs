mod context;
mod latency;
mod legacy;
mod rewrite;
mod service;
mod upstream;

pub use legacy::serve_legacy_ping;
pub use service::{read_favicon_data_url, render_local_json, serve};
