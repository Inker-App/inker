#!/bin/sh
# Install scripts/check-pii.py as this clone's git pre-commit hook.
# Usage: sh scripts/install-hooks.sh   (from anywhere inside the repo)
set -eu

root=$(git rev-parse --show-toplevel)
hooks=$(git -C "$root" rev-parse --path-format=absolute --git-path hooks)
mkdir -p "$hooks"
chmod +x "$root/scripts/check-pii.py"

if [ -e "$hooks/pre-commit" ] && [ ! -L "$hooks/pre-commit" ]; then
  mv "$hooks/pre-commit" "$hooks/pre-commit.backup"
  echo "install-hooks: moved existing pre-commit hook to pre-commit.backup"
fi
ln -sf "$root/scripts/check-pii.py" "$hooks/pre-commit"
echo "install-hooks: pre-commit -> scripts/check-pii.py"
