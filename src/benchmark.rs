#![cfg_attr(not(feature = "benchmark"), allow(dead_code, unused_imports))]

pub mod harness;
pub mod scenarios;

#[cfg(feature = "benchmark")]
pub use harness::{RelayHarness, RelayImplementation, TrafficBurst, TrafficPlan};
#[cfg(feature = "benchmark")]
pub use scenarios::{
    register_kernel_benchmark, register_mc_benchmark, register_relay_compare_benchmark,
};
