#!/usr/bin/env bash
# 构建 Android APK：cargo ndk 编 Rust -> 拷贝 .so -> Gradle 打包
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
: "${ANDROID_HOME:=${LOCALAPPDATA:-$HOME}/Android/Sdk}"
: "${ANDROID_NDK_HOME:=$(ls -d "$ANDROID_HOME"/ndk/* | tail -1)}"
export ANDROID_HOME ANDROID_NDK_HOME
export ANDROID_JAR="$ANDROID_HOME/platforms/$(ls "$ANDROID_HOME/platforms" | tail -1)/android.jar"
export ANDROID_D8_JAR="$ANDROID_HOME/build-tools/$(ls "$ANDROID_HOME/build-tools" | tail -1)/lib/d8.jar"

ABIS="${ABIS:-arm64-v8a}"
PROFILE="${PROFILE:-release}"
CARGO_FLAGS=""
[ "$PROFILE" = "release" ] && CARGO_FLAGS="--release"

# Slint 的 Java helper 需要 javac + d8
"$ROOT/scripts/prepare-jni.sh" "$ABIS" $CARGO_FLAGS
