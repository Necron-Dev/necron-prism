#![cfg(feature = "benchmark")]

use criterion::Criterion;
use necron_prism::benchmark::{
    register_kernel_benchmark, register_mc_benchmark, register_relay_compare_benchmark,
};
use std::env;

fn main() {
    let benchmark = parse_benchmark_name();
    let mut criterion = Criterion::default();

    match benchmark.as_str() {
        "mc" | "mc_bench" => register_mc_benchmark(&mut criterion),
        "kernel" | "kernel_bench" => register_kernel_benchmark(&mut criterion),
        "compare" | "relay_compare_bench" => register_relay_compare_benchmark(&mut criterion),
        other => {
            eprintln!("error: unknown benchmark '{other}'. Expected one of: mc, kernel, compare");
            std::process::exit(2);
        }
    }

    criterion.final_summary();
}

fn parse_benchmark_name() -> String {
    let mut args = env::args().skip(1);

    while let Some(arg) = args.next() {
        if arg == "--benchmark" {
            if let Some(value) = args.next() {
                return value;
            }
            eprintln!("error: --benchmark requires a value");
            std::process::exit(2);
        }
    }

    eprintln!("error: missing required --benchmark <mc|kernel|compare> argument");
    std::process::exit(2);
}
