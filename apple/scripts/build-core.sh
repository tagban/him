#!/usr/bin/env bash
# Builds the Rust core (him-ffi) for the Mac, iPhone and the iPhone Simulator, packs the
# three into apple/HIMCoreFFI.xcframework, and regenerates the Swift bindings.
#   scripts/build-core.sh          all three
#   scripts/build-core.sh mac      just the Mac (quicker while working)
set -euo pipefail
cd "$(dirname "$0")/../.."
export MACOSX_DEPLOYMENT_TARGET=14.0 IPHONEOS_DEPLOYMENT_TARGET=17.0
targets=(aarch64-apple-darwin x86_64-apple-darwin)
[ "${1:-}" = mac ] || targets+=(aarch64-apple-ios aarch64-apple-ios-sim)
for t in "${targets[@]}"; do
  cargo build -p him-ffi --release --lib --target "$t"
done
cargo run -q -p him-ffi --release --bin uniffi-bindgen -- generate \
  --library target/aarch64-apple-darwin/release/libhimffi.a --language swift --out-dir target/uniffi-swift
cp target/uniffi-swift/HIMCore.swift apple/Sources/HIMCore/

# Headers for every slice: the C header and a module map named for SwiftPM/Xcode.
H=target/uniffi-headers
rm -rf "$H" && mkdir -p "$H"
cp target/uniffi-swift/HIMCoreFFI.h "$H/"
printf 'module HIMCoreFFI {\n    header "HIMCoreFFI.h"\n    export *\n}\n' > "$H/module.modulemap"

mkdir -p target/universal-macos
lipo -create target/aarch64-apple-darwin/release/libhimffi.a target/x86_64-apple-darwin/release/libhimffi.a \
  -output target/universal-macos/libhimffi.a
args=(-library target/universal-macos/libhimffi.a -headers "$H")
if [ "${1:-}" != mac ]; then
  args+=(-library target/aarch64-apple-ios/release/libhimffi.a -headers "$H")
  args+=(-library target/aarch64-apple-ios-sim/release/libhimffi.a -headers "$H")
fi
rm -rf apple/HIMCoreFFI.xcframework
xcodebuild -create-xcframework "${args[@]}" -output apple/HIMCoreFFI.xcframework > /dev/null
echo "core built: apple/HIMCoreFFI.xcframework (${targets[*]})"
