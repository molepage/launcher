#!/bin/zsh
# Build build/Launcher.app: Rust core (static lib) + Swift shell.
#   scripts/build.sh            build
#   scripts/build.sh --install  also copy to ~/Applications and restart it
set -euo pipefail
cd "${0:A:h}/.."

MACOSX_DEPLOYMENT_TARGET=13.0 cargo build --release -p launcher-core

APP=build/Launcher.app
rm -rf "$APP"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"
swiftc -O -swift-version 5 -target arm64-apple-macos13.0 \
  -import-objc-header shell-macos/bridge.h \
  shell-macos/*.swift \
  -L target/release -llauncher_core \
  -framework AppKit -framework Carbon -framework QuickLookThumbnailing \
  -o "$APP/Contents/MacOS/Launcher"
cp shell-macos/Info.plist "$APP/Contents/"
cp -R addons "$APP/Contents/Resources/addons"

# Sign with a real identity if there is one (keeps Accessibility permission
# across rebuilds); otherwise ad-hoc.
IDENTITY=${LAUNCHER_SIGN_IDENTITY:-$(security find-identity -v -p codesigning 2>/dev/null | awk -F'"' '/Apple Development|Developer ID Application/ {print $2; exit}')}
codesign --force --deep --sign "${IDENTITY:--}" "$APP" 2>&1 | grep -v "replacing existing signature" || true
echo "built $APP ($(du -sh "$APP" | cut -f1)), signed with ${IDENTITY:-ad-hoc}"

if [[ ${1:-} == --install ]]; then
  pkill -x Launcher 2>/dev/null || true
  mkdir -p ~/Applications
  rm -rf ~/Applications/Launcher.app
  cp -R "$APP" ~/Applications/
  open ~/Applications/Launcher.app
  echo "installed ~/Applications/Launcher.app"
fi
