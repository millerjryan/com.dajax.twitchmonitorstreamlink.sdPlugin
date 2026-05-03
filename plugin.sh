#!/usr/bin/env bash
# Twitch Monitor StreamDock Plugin — Unix launcher (Rust binary)
# Stream Deck / StreamDock / OpenDeck software invokes this with:
#   -port <n> -pluginUUID <uuid> -registerEvent <event> -info <json>

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

# Detect OS and architecture, then pick the appropriate binary
OS="$(uname -s)"
ARCH="$(uname -m)"

if [ "$OS" = "Linux" ]; then
  PLUGIN_BIN="$SCRIPT_DIR/bin/linux-x64/twitchmonitor-plugin"
elif [ "$ARCH" = "arm64" ]; then
  PLUGIN_BIN="$SCRIPT_DIR/bin/mac-arm64/twitchmonitor-plugin"
else
  PLUGIN_BIN="$SCRIPT_DIR/bin/mac-x64/twitchmonitor-plugin"
fi

if [ ! -x "$PLUGIN_BIN" ]; then
  echo "[TwitchMonitor] ERROR: Rust binary not found at $PLUGIN_BIN" >&2
  echo "[TwitchMonitor] Build with: cargo build --release --manifest-path rust/Cargo.toml" >&2
  echo "[TwitchMonitor] Then copy target/release/twitchmonitor-plugin to bin/linux-x64/ (Linux) or bin/mac-x64/ / bin/mac-arm64/ (macOS)" >&2
  exit 1
fi

exec "$PLUGIN_BIN" "$@"
