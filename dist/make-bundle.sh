#!/bin/sh
# Build the Claude Desktop Extension (INKER.mcpb) with `inker bundle`.
#
#   dist/make-bundle.sh [OUT] [VERSION]
#
# OUT defaults to dist/INKER.mcpb, VERSION to the binary's own version.
# The binary bundled is $INKER_BIN, default core/target/release/inker.
#
# For a release the binary must be signed with a Developer ID and notarized
# BEFORE it goes into the bundle (signing rewrites the file). That is Ben's
# step on the Mac; see dist/README.md.
set -eu

here=$(cd "$(dirname "$0")" && pwd)
bin=${INKER_BIN:-$here/../core/target/release/inker}
out=${1:-$here/INKER.mcpb}
version=${2:-}

if [ ! -x "$bin" ]; then
    echo "make-bundle: no inker binary at $bin" >&2
    echo "make-bundle: build it (cd core && cargo build --release -p inker) or set INKER_BIN" >&2
    exit 1
fi

if command -v codesign >/dev/null 2>&1 && codesign --verify --strict "$bin" 2>/dev/null; then
    :
else
    echo "make-bundle: warning: $bin is not signed with a Developer ID; this bundle is for testing only" >&2
fi

if [ -n "$version" ]; then
    "$bin" bundle --out "$out" --version "$version"
else
    "$bin" bundle --out "$out"
fi
