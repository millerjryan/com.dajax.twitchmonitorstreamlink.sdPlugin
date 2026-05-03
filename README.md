# Twitch Monitor StreamDock Plugin

A [StreamDock](https://www.ajazz.com/pages/streamdock) / [OpenDeck](https://github.com/nekename/OpenDeck) / Stream Deck plugin written in Rust that monitors Twitch channel live status directly on your keypad buttons.

**Version:** 1.3.0  
**Author:** dajax  
**Platform:** Windows · macOS · Linux

---

## Features

### Twitch Monitor (keypad action)
- Displays the channel's profile picture on the button — **full colour when live**, **greyscale when offline**
- Overlays a live viewer count banner at the top of the image when the channel is live
- Shows a bold red **LIVE** badge at the bottom-right corner
- Optional alert sound when a channel goes live (upload any MP3 in the property inspector)
- Button click action: **open in browser** or **launch via Streamlink**

### Follows Live Counter (keypad action)
- Displays how many channels you follow are currently live
- **Press the button to instantly switch to any configured device profile** — enter the profile name in the property inspector settings
- Profile switching is compatible with OpenDeck, StreamDock, and Elgato Stream Deck software

### Live Channel by Index (keypad action)
- Displays the **Nth live channel** from your followed channels list, ordered by current viewer count (highest first)
- Index 1 = most-watched live channel you follow, index 2 = second most-watched, and so on
- When a channel occupies the slot the button shows the **full-colour avatar**, **LIVE badge**, and **viewer count** — identical to the Twitch Monitor action
- When no live channel exists at the configured index the button displays a **solid black screen**
- Optional alert sound plays whenever a new channel appears in the slot
- Button click action: **open in browser** or **launch via Streamlink**

#### Recommended setup — a dedicated "Live Follows" profile

The most effective way to use this action is to dedicate an entire device profile to your live follows list:

1. **Create a new profile** in your StreamDock / Stream Deck software (e.g. name it `Live Follows`).
2. **Add a Follows Live Counter button** to your main profile and set its *On Click — Switch Profile* value to `Live Follows`. Pressing it will jump straight to the follows page.
3. **Fill the `Live Follows` profile** with Live Channel by Index buttons — assign index **1** to the first button, **2** to the second, **3** to the third, and so on across every key.
4. Each button will automatically track whichever channel currently holds that viewer-count rank among your live follows. Slots with no live channel stay black and do nothing when pressed.

This gives you an at-a-glance live dashboard and lets you jump directly into any of your top followed streams with a single key press.

---

## Requirements

### Runtime
| Platform | Requirement |
|---|---|
| Windows | Windows 7 or later |
| macOS | macOS 10.11 (El Capitan) or later |
| Linux | Any modern x86-64 distribution |
| StreamDock / Stream Deck software | v2.10.179.426 or later |
| OpenDeck | Any recent version |

### OpenDeck — Profile Switching Note
OpenDeck restricts the `switchProfile` WebSocket event to an internal allowlist of plugin UUIDs. This plugin works around that restriction by briefly opening a second connection registered under the `opendeck_alternative_elgato_implementation` UUID (a built-in allowed identity that no other plugin uses), sending the profile switch, then immediately closing it. No existing plugin or connection is affected.

### Streamlink (optional)
If you want button clicks to open streams in [Streamlink](https://streamlink.github.io/) instead of a browser, install it and make sure `streamlink` is on your `PATH`, or provide the full path to the executable in the property inspector.

---

## Building from Source

### Prerequisites
- [Rust](https://rustup.rs/) 1.70 or later (2021 edition)
- Cargo (included with Rust)

### Steps

```powershell
# Windows
cd rust
cargo build --release
```

```bash
# macOS / Linux
cd rust
cargo build --release
```

The compiled binary will be at:
- **Windows:** `rust/target/release/twitchmonitor-plugin.exe`
- **macOS / Linux:** `rust/target/release/twitchmonitor-plugin`

### Deploy the binary

Copy the binary into the correct `bin/` subdirectory so the launcher scripts can find it:

```powershell
# Windows
Copy-Item rust\target\release\twitchmonitor-plugin.exe bin\win-x64\
```

```bash
# macOS — Intel
cp rust/target/release/twitchmonitor-plugin bin/mac-x64/

# macOS — Apple Silicon
cp rust/target/release/twitchmonitor-plugin bin/mac-arm64/

# Linux — x86-64
cp rust/target/release/twitchmonitor-plugin bin/linux-x64/
chmod +x bin/linux-x64/twitchmonitor-plugin
```

---

## Packaging a Distributable Zip

The plugin must be distributed as a `.zip` file whose root folder is named `com.dajax.twitchmonitorstreamlink.sdPlugin`.

Required zip contents:

```
com.dajax.twitchmonitorstreamlink.sdPlugin/
  manifest.json
  plugin.bat                        ← Windows launcher
  plugin.sh                         ← macOS / Linux launcher
  property_inspector.html
  property_inspector_follows.html
  property_inspector_follows_index.html
  airplane-ding-dong.mp3
  images/
    action_icon.png
    icon.png
    icon_offline.png
    key_offline.png
    key_online.png
    twitch.png
  bin/
    win-x64/
      twitchmonitor-plugin.exe
    linux-x64/
      twitchmonitor-plugin
```

PowerShell packaging script (run from the repository root):

```powershell
$root   = "."
$stage  = "$root\_stage\com.dajax.twitchmonitorstreamlink.sdPlugin"
$zipOut = "$root\com.dajax.twitchmonitorstreamlink.sdPlugin.zip"

New-Item -ItemType Directory -Force "$stage\images", "$stage\bin\win-x64", "$stage\bin\linux-x64" | Out-Null

Copy-Item "$root\manifest.json", "$root\plugin.bat", "$root\plugin.sh",
          "$root\property_inspector.html", "$root\property_inspector_follows.html",
          "$root\property_inspector_follows_index.html",
          "$root\airplane-ding-dong.mp3" -Destination "$stage\"

Copy-Item "$root\images\*"  "$stage\images\"
Copy-Item "$root\rust\target\release\twitchmonitor-plugin.exe" "$stage\bin\win-x64\"
# Linux binary (build on Linux or via cross-compilation):
# Copy-Item "$root\rust\target\x86_64-unknown-linux-gnu\release\twitchmonitor-plugin" "$stage\bin\linux-x64\"

Remove-Item $zipOut -ErrorAction SilentlyContinue
Compress-Archive -Path "$root\_stage\*" -DestinationPath $zipOut
Remove-Item -Recurse -Force "$root\_stage"
```

---

## Project Structure

```
com.dajax.twitchmonitorstreamlink.sdPlugin/
  manifest.json                     ← StreamDock plugin manifest
  plugin.bat                        ← Windows entry point
  plugin.sh                         ← macOS / Linux entry point
  property_inspector.html           ← Settings UI for Twitch Monitor action
  property_inspector_follows.html   ← Settings UI for Follows Live Counter action
  property_inspector_follows_index.html ← Settings UI for Live Channel by Index action
  airplane-ding-dong.mp3            ← Default alert sound
  images/                           ← Plugin and button icons
  bin/
    win-x64/                        ← Windows release binary (not in source control)
    linux-x64/                      ← Linux x86-64 binary (not in source control)
    mac-x64/                        ← macOS Intel binary (not in source control)
    mac-arm64/                      ← macOS Apple Silicon binary (not in source control)
  rust/                             ← Rust source
    Cargo.toml
    Cargo.lock
    assets/
      sans.ttf                      ← Segoe UI Regular (embedded in binary)
      sans_bold.ttf                 ← Segoe UI Bold (embedded in binary)
    src/
      main.rs                       ← Entry point, WebSocket event loop, polling
      images.rs                     ← Image generation (avatar, overlays, badges)
      twitch.rs                     ← Twitch Helix API client
      audio.rs                      ← Alert sound playback
      oauth.rs                      ← OAuth callback HTTP server
```

---

## Rust Dependencies

| Crate | Purpose |
|---|---|
| `tokio` | Async runtime (current-thread) |
| `tokio-tungstenite` | WebSocket client for StreamDock SDK |
| `reqwest` | HTTP client for Twitch API and avatar downloads |
| `serde` / `serde_json` | JSON serialisation |
| `image` | Image resizing, compositing, greyscale conversion |
| `ab_glyph` | TTF font rendering (viewer count, LIVE badge) |
| `base64` | PNG image encoding for StreamDock `setImage` events |
| `rand` | OAuth state nonce generation |
| `url` | URL handling |

---

## GitHub Actions — Automated Releases

Pushing a tag matching `v*` triggers the release workflow (`.github/workflows/release.yml`), which compiles the binary and publishes a GitHub Release with the packaged zip attached.

### Current state

| Target | Status |
|---|---|
| `win-x64` (Windows x86-64) | ✅ Enabled |
| `linux-x64` (Linux x86-64) | ✅ Enabled |
| `mac-x64` (macOS Intel) | ⏸ Disabled (see below) |
| `mac-arm64` (macOS Apple Silicon) | ⏸ Disabled (see below) |

### Re-enabling macOS builds

The macOS matrix entries and packaging steps are commented out in the workflow file. To re-enable them:

1. Open `.github/workflows/release.yml`
2. Uncomment the two macOS matrix entries under `jobs.build.strategy.matrix.include`:

```yaml
- target:       mac-x64
  os:           macos-13
  cargo_target: x86_64-apple-darwin
  binary:       twitchmonitor-plugin

- target:       mac-arm64
  os:           macos-14
  cargo_target: aarch64-apple-darwin
  binary:       twitchmonitor-plugin
```

3. Uncomment the `mkdir`, `cp`, and `chmod` lines for `mac-x64` and `mac-arm64` in the `release` job's **Assemble plugin directory** step.

> **Note:** macOS GitHub-hosted runners (`macos-13`, `macos-14`) are drawn from a much smaller shared pool than Windows or Linux runners. Jobs may wait 5–15 minutes in the queue during peak hours before a machine becomes available — this is expected behaviour.

### Cross-compiling macOS binaries on Windows (alternative)

If macOS runner wait times are a concern, it is possible to cross-compile macOS targets from a Linux runner using `cargo-zigbuild` and the Zig toolchain. This requires additional workflow configuration and is not currently set up.

---


The plugin communicates with the Twitch Helix API using an embedded Client ID and secret (XOR-obfuscated at compile time). OAuth authorisation is performed through the property inspector UI — click **Authorise with Twitch** and follow the browser prompt. The redirect lands on a local callback server on port **7878**.

Tokens are stored in the plugin's global settings via the StreamDock SDK and automatically refreshed when they expire.

---

## License

Private / proprietary — all rights reserved.