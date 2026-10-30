#!/bin/sh
# Build the INKER disk image with its styled Finder window. macOS only.
#
#   scripts/make-dmg.sh build/INKER.app dist/INKER-0.1-macos.dmg [build/AppIcon.icns]
#
# Needs dmgbuild on PATH (pip install dmgbuild). The window layout and the
# background come from dmg/ (see scripts/make-dmg-background.py). Signing and
# notarizing the image is the caller's job, as before.
set -eu
APP=$1
OUT=$2
ICNS=${3:-}
HERE=$(cd "$(dirname "$0")/.." && pwd)
DMG_DIR="$HERE/dmg"
command -v dmgbuild >/dev/null 2>&1 || { echo "make-dmg.sh: dmgbuild not on PATH (pip install dmgbuild)" >&2; exit 1; }
[ -d "$APP" ] || { echo "make-dmg.sh: no app at $APP" >&2; exit 1; }
mkdir -p "$(dirname "$OUT")"
set -- -s "$DMG_DIR/settings.py" \
  -D app="$(cd "$(dirname "$APP")" && pwd)/$(basename "$APP")" \
  -D background="$DMG_DIR/background.png" \
  -D layout="$DMG_DIR/layout.json"
if [ -n "$ICNS" ]; then
  set -- "$@" -D icon="$(cd "$(dirname "$ICNS")" && pwd)/$(basename "$ICNS")"
fi
dmgbuild "$@" INKER "$OUT"
hdiutil verify "$OUT"
echo "built $OUT"
