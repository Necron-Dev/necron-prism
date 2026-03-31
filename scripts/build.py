#!/usr/bin/env python3
from common import (
    PROFILES,
    TARGETS,
    TOOLCHAINS,
    build_command,
    current_os_name,
    deploy_main_artifact,
    ensure_runner_available,
    normalize_toolchain,
    parse_key_value_arg,
    print_banner,
    prompt_cpu_level,
    run_command,
    select_menu,
)


def interactive_config():
    print_banner("Necron-Prism Build Wizard")
    target_os = select_menu("Target Operating System:", list(TARGETS.keys()))
    arch = select_menu("Target Architecture:", list(TARGETS[target_os].keys()))
    profile = select_menu("Build Profile:", PROFILES)
    toolchain = normalize_toolchain(select_menu("Rust Toolchain:", TOOLCHAINS))

    print("\n[?] Features (e.g. 'http-api')")
    features = input("Enter features (default: 'default'): ").strip() or "default"

    default_runner = "cross" if target_os != current_os_name() else "cargo"
    runner_options = [default_runner] + [runner for runner in ["cargo", "cross"] if runner != default_runner]
    runner = select_menu("Build tool:", runner_options, default_index=0)

    return {
        "target_os": target_os,
        "arch": arch,
        "profile": profile,
        "toolchain": toolchain,
        "features": features,
        "runner": runner,
        "cpu": prompt_cpu_level(arch),
    }


def cli_config(args):
    target_os = parse_key_value_arg(args, "--os", current_os_name())
    if target_os not in TARGETS:
        print(f"Unknown os: {target_os}")
        raise SystemExit(1)

    arch = parse_key_value_arg(args, "--arch", "amd64")
    if arch not in TARGETS[target_os]:
        print(f"Unknown arch for {target_os}: {arch}")
        raise SystemExit(1)

    return {
        "target_os": target_os,
        "arch": arch,
        "profile": parse_key_value_arg(args, "--profile", "release"),
        "toolchain": normalize_toolchain(parse_key_value_arg(args, "--toolchain", "stable")),
        "features": parse_key_value_arg(args, "--features", "default"),
        "runner": parse_key_value_arg(args, "--runner", "cross" if target_os != current_os_name() else "cargo"),
        "cpu": parse_key_value_arg(args, "--cpu", "x86-64-v3" if "amd64" in arch else "generic"),
    }


def run_build(config):
    ensure_runner_available(config["runner"])
    target = TARGETS[config["target_os"]][config["arch"]]
    cmd = build_command(config["runner"], config["toolchain"])
    cmd.extend(["build", "--target", target])

    if config["profile"] == "release":
        cmd.append("--release")

    if config["features"] != "default":
        cmd.extend(["--features", config["features"]])

    run_command("Build", cmd, config["cpu"])
    print("\nBuild Successful!")
    deploy_main_artifact(target, config["profile"], config["target_os"])


def main():
    args = __import__("sys").argv[1:]
    config = interactive_config() if not args else cli_config(args)
    run_build(config)


if __name__ == "__main__":
    try:
        main()
    except KeyboardInterrupt:
        print("\nAborted.")
        raise SystemExit(0)
