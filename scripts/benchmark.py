#!/usr/bin/env python3
from common import (
    AMD64_LEVELS,
    BENCH_TARGETS,
    TARGETS,
    RUNNERS,
    TOOLCHAINS,
    CPU_LEVEL_MAP,
    build_command,
    current_os_name,
    ensure_runner_available,
    normalize_toolchain,
    parse_key_value_arg,
    print_banner,
    run_command,
    select_menu,
)


def interactive_config():
    print_banner("Necron-Prism Benchmark Wizard")
    bench_key = select_menu("Benchmark target:", list(BENCH_TARGETS.keys()), default_index=0)
    target_os = select_menu("Target Operating System:", list(TARGETS.keys()), default_index=list(TARGETS.keys()).index(current_os_name()) if current_os_name() in TARGETS else 0)
    arch = select_menu("Target Architecture:", list(TARGETS[target_os].keys()), default_index=0)
    toolchain = normalize_toolchain(select_menu("Rust Toolchain:", TOOLCHAINS))
    default_runner = "cross" if target_os != current_os_name() else "cargo"
    runner_options = [default_runner] + [runner for runner in RUNNERS if runner != default_runner]
    runner = select_menu("Runner:", runner_options, default_index=0)
    cpu_level = select_menu("CPU Optimization Level:", AMD64_LEVELS, default_index=2)

    return {
        "bench": bench_key,
        "target_os": target_os,
        "arch": arch,
        "target": TARGETS[target_os][arch],
        "toolchain": toolchain,
        "runner": runner,
        "cpu": CPU_LEVEL_MAP[cpu_level],
    }


def cli_config(args):
    bench = parse_key_value_arg(args, "--bench")
    if bench is None:
        print("Missing required --bench argument")
        raise SystemExit(1)

    if bench in BENCH_TARGETS:
        bench = bench
    elif bench not in BENCH_TARGETS.values():
        print(f"Unknown benchmark target: {bench}")
        print(f"Available targets: {', '.join(BENCH_TARGETS.keys())} / {', '.join(BENCH_TARGETS.values())}")
        raise SystemExit(1)
    else:
        reverse_map = {value: key for key, value in BENCH_TARGETS.items()}
        bench = reverse_map[bench]

    target_os = parse_key_value_arg(args, "--os", current_os_name())
    if target_os not in TARGETS:
        print(f"Unknown os: {target_os}")
        raise SystemExit(1)

    arch = parse_key_value_arg(args, "--arch", "amd64")
    if arch not in TARGETS[target_os]:
        print(f"Unknown arch for {target_os}: {arch}")
        raise SystemExit(1)

    return {
        "bench": bench,
        "target_os": target_os,
        "arch": arch,
        "target": parse_key_value_arg(args, "--target", TARGETS[target_os][arch]),
        "toolchain": normalize_toolchain(parse_key_value_arg(args, "--toolchain", "stable")),
        "runner": parse_key_value_arg(
            args,
            "--runner",
            "cross" if target_os != current_os_name() else "cargo",
        ),
        "cpu": parse_key_value_arg(args, "--cpu", "x86-64-v3" if "amd64" in arch else "generic"),
    }


def run_benchmark(config):
    ensure_runner_available(config["runner"])
    cmd = build_command(config["runner"], config["toolchain"])
    cmd.extend([
        "run",
        "--target",
        config["target"],
        "--release",
        "--features",
        "benchmark",
        "--bin",
        "run_benchmark",
        "--",
        "--benchmark",
        config["bench"],
    ])
    run_command(f"Bench target: {config['target']}", cmd, config["cpu"])
    print("\nBenchmark Successful!")


def main():
    args = __import__("sys").argv[1:]
    config = interactive_config() if not args else cli_config(args)
    run_benchmark(config)


if __name__ == "__main__":
    try:
        main()
    except KeyboardInterrupt:
        print("\nAborted.")
        raise SystemExit(0)
