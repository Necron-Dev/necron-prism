#![cfg_attr(not(feature = "benchmark"), allow(dead_code, unused_imports))]

#[cfg(feature = "benchmark")]
mod imp {
    use super::super::harness::{RelayHarness, RelayImplementation, TrafficBurst, TrafficPlan};
    use criterion::{BenchmarkId, Criterion, SamplingMode, Throughput};
    use std::env;
    use std::time::Duration;

    const KERNEL_BUFFER_SIZE: usize = 1024 * 1024;
    const RELAY_BUFFER_SIZE: usize = 1024 * 1024;
    const SESSION_BUFFER_SIZE: usize = 512 * 1024;

    const KERNEL_STREAM_FRAME_SIZE: usize = 1024 * 1024;
    const KERNEL_STREAM_FRAMES: usize = 64;

    const SESSION_BULK_FRAME_SIZE: usize = 512 * 1024;
    const SESSION_BULK_FRAMES: usize = 32;

    const RELAY_STREAM_FRAME_SIZE: usize = 256 * 1024;
    const RELAY_STREAM_FRAMES: usize = 16;
    const RELAY_WARM_CONTROL_SIZE: usize = 16 * 1024;
    const RELAY_WARM_CONTROL_PACKETS: usize = 8;
    const RELAY_IMPL_ENV: &str = "NECRON_RELAY_IMPL";

    pub fn register_kernel_benchmark(c: &mut Criterion) {
        let plan = kernel_stream_plan();
        let harness = RelayHarness::new(KERNEL_BUFFER_SIZE);
        harness.warm_up(&plan, 1);

        let mut group = c.benchmark_group("prism_kernel_benchmark");
        group.sampling_mode(SamplingMode::Flat);
        group.sample_size(10);
        group.warm_up_time(Duration::from_millis(200));
        group.measurement_time(Duration::from_secs(1));
        group.throughput(Throughput::Bytes(plan.total_bytes()));
        group.bench_function(plan.label(), |b| {
            b.iter(|| harness.run_plan(&plan));
        });
        group.finish();
    }

    pub fn register_mc_benchmark(c: &mut Criterion) {
        let plan = player_session_plan();
        let harness = RelayHarness::new(SESSION_BUFFER_SIZE);
        harness.warm_up(&plan, 1);

        let mut group = c.benchmark_group("prism_session_benchmark");
        group.sampling_mode(SamplingMode::Flat);
        group.sample_size(10);
        group.warm_up_time(Duration::from_millis(200));
        group.measurement_time(Duration::from_secs(1));
        group.noise_threshold(0.03);
        group.throughput(Throughput::Bytes(plan.total_bytes()));
        group.bench_function(plan.label(), |b| {
            b.iter(|| harness.run_plan(&plan));
        });
        group.finish();
    }

    pub fn register_relay_compare_benchmark(c: &mut Criterion) {
        let plan = relay_benchmark_plan();
        let mut group = c.benchmark_group("prism_relay_benchmark");
        group.sampling_mode(SamplingMode::Flat);
        group.sample_size(10);
        group.warm_up_time(Duration::from_millis(100));
        group.measurement_time(Duration::from_millis(500));
        group.throughput(Throughput::Bytes(plan.total_bytes()));

        for (name, implementation) in [
            ("sync_relay", RelayImplementation::Sync),
            ("plain_copy_relay", RelayImplementation::PlainCopy),
            ("tokio_async_relay", RelayImplementation::TokioAsync),
            ("custom_relay", RelayImplementation::CustomRelay),
            ("prism_relay", RelayImplementation::Prism),
        ] {
            if let Some(selected) = selected_relay_impl() {
                if selected != name {
                    continue;
                }
            }

            let harness = RelayHarness::with_impl(RELAY_BUFFER_SIZE, implementation);
            harness.warm_up(&plan, 1);
            group.bench_with_input(
                BenchmarkId::new(plan.label(), name),
                &harness,
                |b, harness| {
                    b.iter(|| harness.run_plan(&plan));
                },
            );
        }

        group.finish();
    }

    fn kernel_stream_plan() -> TrafficPlan {
        TrafficPlan::new(
            "kernel_bulk_stream",
            vec![TrafficBurst::new(
                pattern_payload(KERNEL_STREAM_FRAME_SIZE, 0x57),
                KERNEL_STREAM_FRAMES,
            )],
        )
    }

    fn player_session_plan() -> TrafficPlan {
        TrafficPlan::new(
            "session_bulk_stream",
            vec![TrafficBurst::new(
                pattern_payload(SESSION_BULK_FRAME_SIZE, 0x63),
                SESSION_BULK_FRAMES,
            )],
        )
    }

    fn relay_benchmark_plan() -> TrafficPlan {
        TrafficPlan::new(
            "relay_bulk_stream",
            vec![
                TrafficBurst::new(
                    pattern_payload(RELAY_WARM_CONTROL_SIZE, 0x18),
                    RELAY_WARM_CONTROL_PACKETS,
                ),
                TrafficBurst::new(
                    pattern_payload(RELAY_STREAM_FRAME_SIZE, 0x91),
                    RELAY_STREAM_FRAMES,
                ),
            ],
        )
    }

    fn pattern_payload(size: usize, seed: u8) -> Vec<u8> {
        let mut payload = Vec::with_capacity(size);
        for index in 0..size {
            payload.push(seed.wrapping_add((index % 251) as u8));
        }
        payload
    }

    fn selected_relay_impl() -> Option<String> {
        env::var(RELAY_IMPL_ENV)
            .ok()
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
    }
}

#[cfg(feature = "benchmark")]
pub use imp::*;
