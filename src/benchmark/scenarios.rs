use super::harness::{RelayHarness, RelayImplementation};
use criterion::{BenchmarkId, Criterion, Throughput};
use std::time::Duration;

const KERNEL_BUFFER_SIZE: usize = 128 * 1024;
const KERNEL_PAYLOAD_SIZE: usize = 16 * 1024 * 1024;
const KERNEL_REPEATS: usize = 8;

const MC_BUFFER_SIZE: usize = 32 * 1024;
const MC_MOVE_PACKET_SIZE: usize = 50;
const MC_CHAT_PACKET_SIZE: usize = 200;
const MC_CHUNK_PACKET_SIZE: usize = 8192;
const MC_MIXED_ITERATIONS: usize = 12_000;

const RELAY_COMPARE_REPEATS: usize = 4;

pub fn register_kernel_benchmark(c: &mut Criterion) {
    let mut group = c.benchmark_group("prism_kernel");
    group.measurement_time(Duration::from_secs(5));
    group.sample_size(10);

    let harness = RelayHarness::new(KERNEL_BUFFER_SIZE);
    let payload = vec![0xAB_u8; KERNEL_PAYLOAD_SIZE];
    group.throughput(Throughput::Bytes(
        (KERNEL_PAYLOAD_SIZE * KERNEL_REPEATS) as u64,
    ));
    group.bench_function("relay_max_throughput", |b| {
        b.iter(|| {
            harness.run_bytes(&payload, KERNEL_REPEATS, KERNEL_BUFFER_SIZE);
        });
    });

    group.finish();
}

pub fn register_mc_benchmark(c: &mut Criterion) {
    let mut group = c.benchmark_group("prism_mc_realistic");
    group.sample_size(20);
    group.warm_up_time(Duration::from_secs(2));
    group.measurement_time(Duration::from_secs(8));
    group.noise_threshold(0.05);

    let harness = RelayHarness::new(MC_BUFFER_SIZE);
    let move_data = vec![0x20_u8; MC_MOVE_PACKET_SIZE];
    let chat_data = vec![0x02_u8; MC_CHAT_PACKET_SIZE];
    let chunk_data = vec![0x10_u8; MC_CHUNK_PACKET_SIZE];
    let mut payload =
        Vec::with_capacity(MC_MOVE_PACKET_SIZE + MC_CHAT_PACKET_SIZE + MC_CHUNK_PACKET_SIZE);
    payload.extend_from_slice(&move_data);
    payload.extend_from_slice(&chat_data);
    payload.extend_from_slice(&chunk_data);

    group.throughput(Throughput::Bytes(
        (MC_MIXED_ITERATIONS * payload.len()) as u64,
    ));
    group.bench_function("real_mc_session", |b| {
        b.iter(|| {
            harness.run_bytes(&payload, MC_MIXED_ITERATIONS, MC_BUFFER_SIZE);
        });
    });

    group.finish();
}

pub fn register_relay_compare_benchmark(c: &mut Criterion) {
    let mut group = c.benchmark_group("relay_compare");
    group.sample_size(12);
    group.warm_up_time(Duration::from_secs(2));
    group.measurement_time(Duration::from_secs(6));

    let payload = vec![0xAB_u8; KERNEL_PAYLOAD_SIZE];
    group.throughput(Throughput::Bytes(
        (KERNEL_PAYLOAD_SIZE * RELAY_COMPARE_REPEATS) as u64,
    ));

    for (name, implementation) in [
        ("standard_relay", RelayImplementation::Standard),
        ("custom_async_relay", RelayImplementation::CustomAsync),
        ("sync_relay", RelayImplementation::Sync),
    ] {
        let harness = RelayHarness::with_impl(KERNEL_BUFFER_SIZE, implementation);
        group.bench_with_input(
            BenchmarkId::new("throughput", name),
            &implementation,
            |b, _| {
                b.iter(|| {
                    harness.run_bytes(&payload, RELAY_COMPARE_REPEATS, KERNEL_BUFFER_SIZE);
                });
            },
        );
    }

    group.finish();
}
