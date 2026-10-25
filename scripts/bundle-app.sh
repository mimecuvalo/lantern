#!/bin/bash
# Build Lantern and wrap it in Lantern.app, signed ad hoc.
#
# Usage: scripts/bundle-app.sh [--release] [--open] [-- lantern args...]
#
#   --release  build optimised (default: debug)
#   --open     then launch it with `open`, as Finder would; its output goes
#              to target/<profile>/Lantern.log
#
# Running as an app, not a bare binary from a terminal, is what lets Lantern
# reach Apple Music: its helper only answers apps with a bundle identifier
# that macOS treats as responsible for themselves. LANTERN_CONFIG_DIR, if
# set, is passed through.
set -euo pipefail

PROJECT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
PROFILE=debug
OPEN=0
ARGS=()
while [ $# -gt 0 ]; do
  case "$1" in
    --release) PROFILE=release ;;
    --open) OPEN=1 ;;
    --) shift; ARGS=("$@"); break ;;
    *) echo "unknown option: $1" >&2; exit 2 ;;
  esac
  shift
done

cd "$PROJECT_DIR"
if [ "$PROFILE" = release ]; then
  cargo build --release
else
  cargo build
fi

VERSION="$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)"
APP="$PROJECT_DIR/target/$PROFILE/Lantern.app"
rm -rf "$APP"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"
cp "target/$PROFILE/lantern" "$APP/Contents/MacOS/lantern"
cp macos/AppIcon.icns "$APP/Contents/Resources/AppIcon.icns"
sed "s/@VERSION@/$VERSION/g" macos/Info.plist > "$APP/Contents/Info.plist"
codesign --force --sign - "$APP" >/dev/null 2>&1
echo "OK  $APP"

if [ "$OPEN" = 1 ]; then
  LOG="$PROJECT_DIR/target/$PROFILE/Lantern.log"
  ENV=()
  if [ -n "${LANTERN_CONFIG_DIR:-}" ]; then
    ENV=(--env "LANTERN_CONFIG_DIR=$LANTERN_CONFIG_DIR")
  fi
  open -n "${ENV[@]+"${ENV[@]}"}" --stdout "$LOG" --stderr "$LOG" "$APP" \
    ${ARGS[@]+--args "${ARGS[@]}"}
  echo "launched; output in $LOG"
fi
