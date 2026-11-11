#!/usr/bin/env bash
# 编译 Rust 动态库并复制到 android/app/src/main/jniLibs
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
ABIS="${1:-arm64-v8a}"
shift || true

cd "$ROOT"
mkdir -p crates/app/android/app/src/main/jniLibs
cargo ndk -t "$ABIS" -P 24 -o crates/app/android/app/src/main/jniLibs "$@" build -p jhentai "$@"
find crates/app/android/app/src/main/jniLibs -name '*.so' -exec ls -la {} \;
