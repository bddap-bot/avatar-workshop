#!/usr/bin/env bash
# Builds the extension for this machine into addons/avatar_workshop/bin/, where avatar_core.gdextension finds it.
set -euo pipefail
root=$(cd "$(dirname "$0")/.." && pwd)
cargo build --release --manifest-path "$root/avatar-godot/Cargo.toml"
arch=$(uname -m)
dir=$root/addons/avatar_workshop/bin/linux-${arch/aarch64/arm64}
mkdir -p "$dir"
cp "${CARGO_TARGET_DIR:-$root/avatar-godot/target}/release/libavatar_godot.so" "$dir/"
