#!/usr/bin/env bash
# Twitch Monitor StreamDock Plugin — macOS launcher (Rust binary)
# Stream Deck / StreamDock software invokes this with:
#   -port <n> -pluginUUID <uuid> -registerEvent <event> -info <json>

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

# Detect architecture and pick the appropriate binary
ARCH="$(uname -m)"
if [ "$ARCH" = "arm64" ]; then
  PLUGIN_BIN="$SCRIPT_DIR/bin/mac-arm64/twitchmonitor-plugin"
else
  PLUGIN_BIN="$SCRIPT_DIR/bin/mac-x64/twitchmonitor-plugin"
fi

if [ ! -x "$PLUGIN_BIN" ]; then
  echo "[TwitchMonitor] ERROR: Rust binary not found at $PLUGIN_BIN" >&2
  echo "[TwitchMonitor] Build with: cargo build --release --manifest-path rust/Cargo.toml" >&2
  echo "[TwitchMonitor] Then copy target/release/twitchmonitor-plugin to bin/mac-x64/twitchmonitor-plugin (or mac-arm64/twitchmonitor-plugin)" >&2
  exit 1
fi

exec "$PLUGIN_BIN" "$@"
