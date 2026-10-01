#!/usr/bin/env bash
# Builds the Rust core for macOS and regenerates its Swift bindings (Sources/HIMCore).
set -euo pipefail
cd "$(dirname "$0")/../.."
export MACOSX_DEPLOYMENT_TARGET=14.0
cargo build -p him-ffi --release
cargo run -q -p him-ffi --release --bin uniffi-bindgen -- generate \
  --library target/release/libhimffi.a --language swift --out-dir target/uniffi-swift
cp target/uniffi-swift/HIMCore.swift apple/Sources/HIMCore/
cp target/uniffi-swift/HIMCoreFFI.h apple/Sources/HIMCoreFFI/include/
echo "core built: target/release/libhimffi.a"
