#![cfg(feature = "benchmark")]

use criterion::Criterion;
use necron_prism::benchmark::{
    register_kernel_benchmark, register_mc_benchmark, register_relay_compare_benchmark,
};
use std::env;
use std::process::Command;

const BENCHMARK_NAME_ENV: &str = "NECRON_BENCHMARK";

fn main() {
    let benchmark = match normalize_cli_args() {
        CliMode::Run { benchmark } => benchmark,
        CliMode::Reexec(status) => std::process::exit(status.code().unwrap_or(1)),
    };
    let mut criterion = Criterion::default().configure_from_args();

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

enum CliMode {
    Run { benchmark: String },
    Reexec(std::process::ExitStatus),
}

fn normalize_cli_args() -> CliMode {
    let mut benchmark = env::var(BENCHMARK_NAME_ENV)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());
    let program = env::current_exe().unwrap_or_else(|error| {
        eprintln!("error: failed to locate current executable: {error}");
        std::process::exit(2);
    });
    let mut forwarded_args = Vec::new();
    let mut args = env::args_os().skip(1);

    while let Some(arg) = args.next() {
        if arg == "--benchmark" {
            let Some(value) = args.next() else {
                eprintln!("error: --benchmark requires a value");
                std::process::exit(2);
            };

            let value = value.to_string_lossy().trim().to_string();
            if value.is_empty() {
                eprintln!("error: --benchmark requires a non-empty value");
                std::process::exit(2);
            }
            benchmark = Some(value);
            continue;
        }

        forwarded_args.push(arg);
    }

    let Some(benchmark) = benchmark else {
        eprintln!(
        "error: missing benchmark selection. Set {BENCHMARK_NAME_ENV}=<mc|kernel|compare> or pass --benchmark <mc|kernel|compare>"
        );
        std::process::exit(2);
    };

    if env::var_os(BENCHMARK_NAME_ENV).is_none() {
        let status = Command::new(program)
            .args(&forwarded_args)
            .env(BENCHMARK_NAME_ENV, &benchmark)
            .status()
            .unwrap_or_else(|error| {
                eprintln!("error: failed to restart benchmark process: {error}");
                std::process::exit(2);
            });
        return CliMode::Reexec(status);
    }

    CliMode::Run { benchmark }
}
