#!/usr/bin/env bash
# Build inker-ffi for macOS, generate the Swift bindings, and assemble
# app/Frameworks/InkerCoreFFI.xcframework for Package.swift's binary target.
#
#   app/scripts/build-rust.sh                # arm64 only, release
#   app/scripts/build-rust.sh --universal    # arm64 + x86_64, lipo'd
#   app/scripts/build-rust.sh --debug        # debug profile
#   app/scripts/build-rust.sh --bindings-only
#       Build for the host and regenerate the Swift bindings only, no
#       xcframework. Works on Linux; that is how the committed bindings in
#       app/Sources/InkerCore were first produced.
#
# Outputs:
#   app/Sources/InkerCore/InkerCore.swift             (generated, committed)
#   app/Sources/InkerCore/include/InkerCoreFFI.h      (generated, committed)
#   app/Sources/InkerCore/include/module.modulemap    (generated, committed)
#   app/Frameworks/InkerCoreFFI.xcframework           (built, git-ignored)
set -euo pipefail

universal=0
profile=release
bindings_only=0
for arg in "$@"; do
  case "$arg" in
    --universal) universal=1 ;;
    --debug) profile=debug ;;
    --bindings-only) bindings_only=1 ;;
    -h|--help) sed -n '2,20p' "$0"; exit 0 ;;
    *) echo "unknown option: $arg" >&2; exit 2 ;;
  esac
done

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
app="$(cd "$here/.." && pwd)"
core="$(cd "$app/../core" && pwd)"
out_swift="$app/Sources/InkerCore"
out_headers="$out_swift/include"
xcframework="$app/Frameworks/InkerCoreFFI.xcframework"

export PATH="$HOME/.cargo/bin:$PATH"
cargo_profile_flag=()
[ "$profile" = release ] && cargo_profile_flag=(--release)

host_os="$(uname -s)"
if [ "$host_os" != Darwin ]; then
  bindings_only=1
fi

# Matches Package.swift's platform floor so the linker does not warn about
# objects built for a newer macOS than the app.
export MACOSX_DEPLOYMENT_TARGET="${MACOSX_DEPLOYMENT_TARGET:-26.0}"

if [ "$bindings_only" = 1 ]; then
  targets=()
  (cd "$core" && cargo build -p inker-ffi "${cargo_profile_flag[@]}")
  case "$host_os" in
    Darwin) lib_for_bindgen="$core/target/$profile/libinker_ffi.dylib" ;;
    *) lib_for_bindgen="$core/target/$profile/libinker_ffi.so" ;;
  esac
else
  targets=(aarch64-apple-darwin)
  [ "$universal" = 1 ] && targets+=(x86_64-apple-darwin)
  for t in "${targets[@]}"; do
    if ! rustup target list --installed | grep -qx "$t"; then
      echo "missing Rust target $t; run: rustup target add $t" >&2
      exit 1
    fi
    (cd "$core" && cargo build -p inker-ffi "${cargo_profile_flag[@]}" --target "$t")
  done
  lib_for_bindgen="$core/target/${targets[0]}/$profile/libinker_ffi.dylib"
fi

# Bindings. The bindgen binary lives in inker-ffi itself, so its version always
# matches the scaffolding. Names come from core/crates/inker-ffi/uniffi.toml.
mkdir -p "$out_swift" "$out_headers"
bindgen=(cargo run -q -p inker-ffi --features bindgen --bin uniffi-bindgen-swift --)
(cd "$core" && "${bindgen[@]}" "$lib_for_bindgen" "$out_swift" --swift-sources)
(cd "$core" && "${bindgen[@]}" "$lib_for_bindgen" "$out_headers" --headers --modulemap \
  --module-name InkerCoreFFI --modulemap-filename module.modulemap)
echo "bindings: $out_swift/InkerCore.swift, $out_headers/"

if [ "$bindings_only" = 1 ]; then
  exit 0
fi

# Static library: one arch, or a fat one.
if [ "${#targets[@]}" -gt 1 ]; then
  fat_dir="$core/target/universal-apple-darwin/$profile"
  mkdir -p "$fat_dir"
  libs=()
  for t in "${targets[@]}"; do libs+=("$core/target/$t/$profile/libinker_ffi.a"); done
  lipo -create "${libs[@]}" -output "$fat_dir/libinker_ffi.a"
  static_lib="$fat_dir/libinker_ffi.a"
else
  static_lib="$core/target/${targets[0]}/$profile/libinker_ffi.a"
fi

# The headers dir holds InkerCoreFFI.h and module.modulemap; xcodebuild copies
# both into the xcframework so `import InkerCoreFFI` resolves.
rm -rf "$xcframework"
mkdir -p "$(dirname "$xcframework")"
xcodebuild -create-xcframework \
  -library "$static_lib" \
  -headers "$out_headers" \
  -output "$xcframework"
echo "xcframework: $xcframework"

# What the static lib needs from the system, for Package.swift's linkerSettings.
echo "native static libs needed by inker-ffi (compare with Package.swift linkerSettings):"
(cd "$core" && cargo rustc -q -p inker-ffi --lib "${cargo_profile_flag[@]}" --target "${targets[0]}" \
  --crate-type staticlib -- --print native-static-libs 2>&1 | grep -i 'native-static-libs' || true)
