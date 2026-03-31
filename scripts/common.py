#!/usr/bin/env python3
import os
import shutil
import subprocess
import sys
from pathlib import Path
from functools import lru_cache

TARGETS = {
    "windows": {"amd64": "x86_64-pc-windows-gnu", "v8a": "aarch64-pc-windows-gnullvm"},
    "linux": {
        "amd64": "x86_64-unknown-linux-gnu",
        "armv7": "armv7-unknown-linux-gnueabihf",
        "v8a": "aarch64-unknown-linux-gnu",
        "musl-amd64": "x86_64-unknown-linux-musl",
        "musl-v8a": "aarch64-unknown-linux-musl",
    },
    "macos": {"amd64": "x86_64-apple-darwin", "v8a": "aarch64-apple-darwin"},
    "android": {
        "amd64": "x86_64-linux-android",
        "v8a": "aarch64-linux-android",
        "armv7": "armv7-linux-androideabi",
    },
}

AMD64_LEVELS = ["v1", "v2", "v3", "v4", "native"]
PROFILES = ["release", "dev"]
TOOLCHAINS = ["stable", "beta", "nightly", "none"]
RUNNERS = ["cargo", "cross"]
BENCH_TARGETS = {
    "mc": "mc_bench",
    "kernel": "kernel_bench",
    "compare": "relay_compare_bench",
}
CPU_LEVEL_MAP = {
    "v1": "x86-64",
    "v2": "x86-64-v2",
    "v3": "x86-64-v3",
    "v4": "x86-64-v4",
    "native": "native",
}
REQUIRED_RUSTFLAGS = ["--cfg", "tokio_unstable"]


def print_banner(title):
    print("\n" + "=" * 30)
    print(f" {title}")
    print("=" * 30)


def select_menu(prompt, options, default_index=0):
    print(f"\n[?] {prompt}")
    for i, opt in enumerate(options):
        mark = "*" if i == default_index else " "
        print(f"  {i + 1}) {opt} {mark}")

    while True:
        try:
            choice = input(f"Select (1-{len(options)}) [default: {default_index + 1}]: ").strip()
            if not choice:
                return options[default_index]
            idx = int(choice) - 1
            if 0 <= idx < len(options):
                return options[idx]
        except ValueError:
            pass
        print(f"Invalid input. Please enter a number between 1 and {len(options)}.")


def current_os_name():
    name = sys.platform.replace("win32", "windows").replace("darwin", "macos")
    if "linux" in sys.platform:
        return "linux"
    return name


def prompt_cpu_level(arch):
    if "amd64" not in arch:
        return "generic"
    level = select_menu("CPU Optimization Level (amd64 only):", AMD64_LEVELS, default_index=2)
    return CPU_LEVEL_MAP[level]


def normalize_toolchain(value):
    return None if value == "none" else value


@lru_cache(maxsize=1)
def active_toolchain_name():
    try:
        output = subprocess.check_output(["rustup", "show", "active-toolchain"], text=True).strip()
    except (subprocess.CalledProcessError, FileNotFoundError):
        return None

    if not output:
        return None

    return output.split()[0]


def resolve_toolchain_selector(channel):
    if not channel:
        return None

    active = active_toolchain_name()
    if not active:
        return channel

    channel_prefix = f"{channel}-"
    if active.startswith(channel_prefix):
        return active

    return channel


def ensure_runner_available(runner):
    if runner == "cross" and not shutil.which("cross"):
        print("\n[!] Error: 'cross' not found. Please install it with 'cargo install cross'.")
        sys.exit(1)


def build_command(base_tool, toolchain=None):
    cmd = [base_tool]
    resolved_toolchain = resolve_toolchain_selector(toolchain)
    if resolved_toolchain:
        cmd.append(f"+{resolved_toolchain}")
    return cmd


def run_command(label, cmd, cpu, extra_env=None):
    env = os.environ.copy()
    rustflags_parts = [f"-C target-cpu={cpu}", *REQUIRED_RUSTFLAGS]
    existing_rustflags = env.get("RUSTFLAGS", "").strip()
    if existing_rustflags:
        rustflags_parts.append(existing_rustflags)
    env["RUSTFLAGS"] = " ".join(rustflags_parts).strip()
    if extra_env:
        env.update(extra_env)

    print(f"\n[{label}] CPU: {cpu} | Tool: {cmd[0]}")
    print(f"$ RUSTFLAGS='{env['RUSTFLAGS']}' {' '.join(cmd)}\n")

    try:
        subprocess.run(cmd, env=env, check=True)
    except subprocess.CalledProcessError:
        print(f"\n{label} Failed!")
        sys.exit(1)


def deploy_main_artifact(target, profile, target_os):
    profile_dir = "release" if profile == "release" else "debug"
    bin_ext = ".exe" if target_os == "windows" else ""
    artifact = Path(f"target/{target}/{profile_dir}/necron-prism{bin_ext}")

    if artifact.exists():
        dist_path = Path(f"dist/{target}/necron-prism{bin_ext}")
        dist_path.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(artifact, dist_path)
        print(f"Artifact deployed to: {dist_path}")
    else:
        print(f"[!] Warning: Could not find artifact at {artifact}")


def parse_key_value_arg(args, flag, default=None):
    if flag in args:
        idx = args.index(flag)
        if idx + 1 >= len(args):
            print(f"Missing value for {flag}")
            sys.exit(1)
        return args[idx + 1]
    return default
