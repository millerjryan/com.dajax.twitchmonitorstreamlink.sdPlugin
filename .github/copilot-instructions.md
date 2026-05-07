# Twitch Monitor StreamLink — Copilot Instructions

StreamDeck/OpenDeck plugin written in **Rust** with HTML property inspectors. Monitors Twitch live status and launches streams via Streamlink or browser.

## Build

> ⚠️ **Cargo must be run from the `rust/` subdirectory** — not the repo root (no `Cargo.toml` at root).

```powershell
# Windows (PowerShell)
Push-Location rust; cargo build --release; Pop-Location
```

```bash
# Linux / macOS
cd rust && cargo build --release
```

Output: `rust/target/release/twitchmonitor-plugin.exe` (Windows) or `rust/target/release/twitchmonitor-plugin` (Linux/macOS).

Deploy the binary to `bin/win-x64/` (Windows) or `bin/linux-x64/` (Linux) so `plugin.bat` / `plugin.sh` can find it.

## Release

Tag with `v*` and push — GitHub Actions builds and publishes a release zip automatically:

```bash
git tag vX.Y.Z && git push origin vX.Y.Z
```

See [.github/workflows/release.yml](.github/workflows/release.yml) for the full CI pipeline.

## Architecture

| File | Role |
|------|------|
| [`rust/src/main.rs`](rust/src/main.rs) | WebSocket event loop, polling loops, `AppHandle`, `ButtonSettings`, all `keyDown` handlers |
| [`rust/src/twitch.rs`](rust/src/twitch.rs) | Twitch Helix API — user lookup, stream status, followed live list, token refresh |
| [`rust/src/oauth.rs`](rust/src/oauth.rs) | OAuth 2.0 callback HTTP server on port 7878 |
| [`rust/src/images.rs`](rust/src/images.rs) | Avatar download/resize, greyscale, LIVE badge, viewer count overlay |
| [`rust/src/audio.rs`](rust/src/audio.rs) | MP3 alert playback via rodio |
| [`rust/src/dialog.rs`](rust/src/dialog.rs) | Windows-only WinAPI popup shown while Streamlink is launching |
| [`property_inspector.html`](property_inspector.html) | Settings UI for **Twitch Monitor** action |
| [`property_inspector_follows_index.html`](property_inspector_follows_index.html) | Settings UI for **Live Channel by Index** action |
| [`property_inspector_follows.html`](property_inspector_follows.html) | Settings UI for **Follows Live Counter** action |

## Plugin Actions

| UUID | Name | Description |
|------|------|-------------|
| `com.dajax.twitchmonitorstreamlink.monitor` | Twitch Monitor | Single channel live status; color/greyscale avatar |
| `com.dajax.twitchmonitorstreamlink.followsindex` | Live Channel by Index | Nth live followed channel by viewer count |
| `com.dajax.twitchmonitorstreamlink.followslive` | Follows Live Counter | Count of live followed channels; click to switch profile |

## Settings Flow (Property Inspector ↔ Rust)

- **HTML → Plugin**: `sendToPlugin` WebSocket event with a `cmd` payload  
- **Plugin → HTML**: `sendToPropertyInspector` event  
- **Per-button settings**: `setSettings` / `didReceiveSettings`  
- **Global settings** (OAuth tokens): `setGlobalSettings` / `didReceiveGlobalSettings`  
- **Polling interval**: 60 seconds (constant `POLL_INTERVAL` in `main.rs`)

When adding a new setting:
1. Add the field to `ButtonSettings` in `main.rs` with `#[serde(rename = "camelCase")]`
2. Add the corresponding default value to `manifest.json` under the action's `Settings` block
3. Update the relevant property inspector HTML (input element + read/write in `applySettings` and the save handler)

## Key Pitfalls

- **`cargo` at repo root fails** — always `cd rust` first or use `Push-Location rust`
- **Port 7878** must be free for OAuth; the server binds once at plugin start
- **Streamlink player flag**: invoked as `streamlink --player <vlc|mpv> <url> best` — the player binary must be on `PATH`
- **`StartingDialog`** in `dialog.rs` blocks until the HWND is ready (uses `Condvar`) before returning; `close()` enforces a 3-second minimum display time
- **macOS CI builds are disabled** (commented out in `release.yml`) — uncomment to re-enable
- **Linux audio** requires `libasound2-dev` at build time (installed in CI automatically)
- **XOR-obfuscated credentials** in `main.rs` (`client_id()` / `client_secret()`) — do not replace with plaintext

## Conventions

- Rust: `snake_case` fields with `#[serde(rename = "camelCaseKey")]` to match JSON from the JS property inspectors
- Property inspectors share the same dark CSS theme — keep `--purple`, `--bg`, `--surface`, etc. variables consistent across all HTML files
- New `select` inputs should use the `select { ... }` CSS block already present in each property inspector
- All git tags follow `vMAJOR.MINOR.PATCH` semver
