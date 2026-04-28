@echo off
:: Twitch Monitor StreamDock Plugin — Windows launcher (Rust binary)
:: Stream Deck / StreamDock software invokes this with:
::   -port <n> -pluginUUID <uuid> -registerEvent <event> -info <json>

set "SCRIPT_DIR=%~dp0"
set "PLUGIN_BIN=%SCRIPT_DIR%bin\win-x64\twitchmonitor-plugin.exe"

if not exist "%PLUGIN_BIN%" (
  echo [TwitchMonitor] ERROR: Rust binary not found at %PLUGIN_BIN% >&2
  echo [TwitchMonitor] Build with: cargo build --release --manifest-path rust\Cargo.toml >&2
  echo [TwitchMonitor] Then copy target\release\plugin.exe to bin\win-x64\plugin.exe >&2
  exit /b 1
)

"%PLUGIN_BIN%" %*
