#!/usr/bin/env python3
import argparse
import os
import platform
import shutil
import subprocess
import sys
from pathlib import Path

# 平台和架构映射 (与之前保持一致)
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

def select_menu(prompt, options, default_index=0):
    """纯标准库实现的编号选择菜单"""
    print(f"\n[?] {prompt}")
    for i, opt in enumerate(options):
        mark = "*" if i == default_index else " "
        print(f"  {i+1}) {opt} {mark}")
    
    while True:
        try:
            choice = input(f"Select (1-{len(options)}) [default: {default_index+1}]: ").strip()
            if not choice:
                return options[default_index]
            idx = int(choice) - 1
            if 0 <= idx < len(options):
                return options[idx]
        except ValueError:
            pass
        print(f"Invalid input. Please enter a number between 1 and {len(options)}.")

def get_interactive_config():
    """使用标准输入实现的配置向导"""
    print("\n" + "="*30)
    print(" Necron-Prism Build Wizard")
    print("="*30)
    
    os_name = select_menu("Target Operating System:", list(TARGETS.keys()))
    
    arch_choices = list(TARGETS[os_name].keys())
    arch = select_menu("Target Architecture:", arch_choices)
    
    level = "native"
    if "amd64" in arch:
        level = select_menu("CPU Optimization Level (amd64 only):", AMD64_LEVELS, default_index=2)

    profile = select_menu("Build Profile:", PROFILES)
    
    toolchain = select_menu("Rust Toolchain:", TOOLCHAINS)
    toolchain = None if toolchain == "none" else toolchain

    print(f"\n[?] Features (e.g. 'http-api')")
    features = input("Enter features (default: 'default'): ").strip()
    features = features if features else "default"
    
    return {
        "os": os_name, "arch": arch, "level": level, "profile": profile,
        "toolchain": toolchain, "features": features
    }

def run_build(config):
    """执行构建逻辑"""
    target = TARGETS[config["os"]][config["arch"]]
    
    # 确定 CPU 优化参数
    level_map = {"v1": "x86-64", "v2": "x86-64-v2", "v3": "x86-64-v3", "v4": "x86-64-v4", "native": "native"}
    cpu = level_map.get(config["level"], "native") if "amd64" in config["arch"] else "generic"
    
    # 自动检测是否需要交叉编译工具 (cargo-zigbuild)
    current_os = sys.platform.replace("win32", "windows").replace("darwin", "macos")
    if "linux" in sys.platform: current_os = "linux"
    
    is_cross = config["os"] != current_os
    cargo_bin = "cargo-zigbuild" if (is_cross and shutil.which("cargo-zigbuild")) else "cargo"
    subcmd = "zigbuild" if "zigbuild" in cargo_bin else "build"

    cmd = [cargo_bin]
    if config["toolchain"]:
        cmd.append(f"+{config['toolchain']}")
    cmd.extend([subcmd, "--target", target])
    
    if config["profile"] == "release":
        cmd.append("--release")
        
    if config["features"] and config["features"] != "default":
        cmd.extend(["--features", config["features"]])

    # 设置环境变量
    env = os.environ.copy()
    env["RUSTFLAGS"] = f"-C target-cpu={cpu} {env.get('RUSTFLAGS', '')}".strip()

    print(f"\n[Build] Target: {target} | CPU: {cpu} | Tool: {cargo_bin}")
    print(f"$ RUSTFLAGS='{env['RUSTFLAGS']}' {' '.join(cmd)}\n")
    
    try:
        subprocess.run(cmd, env=env, check=True)
        print("\nBuild Successful! ✅")
        
        # 自动归档到 dist
        profile_dir = "release" if config["profile"] == "release" else "debug"
        bin_ext = ".exe" if config["os"] == "windows" else ""
        artifact = Path(f"target/{target}/{profile_dir}/necron-prism{bin_ext}")
        
        if artifact.exists():
            dist_path = Path(f"dist/{target}/necron-prism{bin_ext}")
            dist_path.parent.mkdir(parents=True, exist_ok=True)
            shutil.copy2(artifact, dist_path)
            print(f"Artifact deployed to: {dist_path}")
            
    except subprocess.CalledProcessError:
        print("\nBuild Failed! ❌")
        sys.exit(1)

def main():
    # 只有在交互式终端且没传参数时才开启向导
    if sys.stdin.isatty() and len(sys.argv) == 1:
        config = get_interactive_config()
    else:
        # 非交互模式下的默认值 (或者你可以继续解析 sys.argv 传参)
        config = {
            "os": "linux", "arch": "amd64", "level": "v3", 
            "profile": "release", "toolchain": "stable", "features": "default"
        }

    run_build(config)

if __name__ == "__main__":
    try:
        main()
    except KeyboardInterrupt:
        print("\nAborted.")
        sys.exit(0)
