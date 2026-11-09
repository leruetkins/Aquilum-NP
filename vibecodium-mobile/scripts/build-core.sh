#!/usr/bin/env bash
# Builds the Rust core for the two ABIs the APK ships and copies the shared
# libraries into app/src/main/jniLibs.
set -euo pipefail
cd "$(dirname "$0")/../core"

cargo build --release --target aarch64-linux-android
cargo build --release --target x86_64-linux-android

mkdir -p ../app/src/main/jniLibs/arm64-v8a ../app/src/main/jniLibs/x86_64
cp target/aarch64-linux-android/release/libvibecodium_core.so ../app/src/main/jniLibs/arm64-v8a/
cp target/x86_64-linux-android/release/libvibecodium_core.so ../app/src/main/jniLibs/x86_64/

echo "core libs copied to app/src/main/jniLibs"
