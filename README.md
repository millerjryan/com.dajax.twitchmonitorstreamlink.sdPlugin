# Twitch Monitor StreamDock Plugin

A [StreamDock](https://www.ajazz.com/pages/streamdock) / Stream Deck plugin written in Rust that monitors Twitch channel live status directly on your keypad buttons.

**Version:** 1.0.1  
**Author:** dajax  
**Platform:** Windows · macOS

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
- Click to switch to a configured StreamDock device profile

---

## Requirements

### Runtime
| Platform | Requirement |
|---|---|
| Windows | Windows 7 or later |
| macOS | macOS 10.11 (El Capitan) or later |
| StreamDock / Stream Deck software | v2.10.179.426 or later |

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
# macOS
cd rust
cargo build --release
```

The compiled binary will be at:
- **Windows:** `rust/target/release/twitchmonitor-plugin.exe`
- **macOS:** `rust/target/release/twitchmonitor-plugin`

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
```

---

## Packaging a Distributable Zip

The plugin must be distributed as a `.zip` file whose root folder is named `com.dajax.twitchmonitorstreamlink.sdPlugin`.

Required zip contents:

```
com.dajax.twitchmonitorstreamlink.sdPlugin/
  manifest.json
  plugin.bat                        ← Windows launcher
  plugin.sh                         ← macOS launcher
  property_inspector.html
  property_inspector_follows.html
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
```

PowerShell packaging script (run from the repository root):

```powershell
$root   = "."
$stage  = "$root\_stage\com.dajax.twitchmonitorstreamlink.sdPlugin"
$zipOut = "$root\com.dajax.twitchmonitorstreamlink.sdPlugin.zip"

New-Item -ItemType Directory -Force "$stage\images", "$stage\bin\win-x64" | Out-Null

Copy-Item "$root\manifest.json", "$root\plugin.bat", "$root\plugin.sh",
          "$root\property_inspector.html", "$root\property_inspector_follows.html",
          "$root\airplane-ding-dong.mp3" -Destination "$stage\"

Copy-Item "$root\images\*"  "$stage\images\"
Copy-Item "$root\rust\target\release\twitchmonitor-plugin.exe" "$stage\bin\win-x64\"

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
  plugin.sh                         ← macOS entry point
  property_inspector.html           ← Settings UI for Twitch Monitor action
  property_inspector_follows.html   ← Settings UI for Follows Live Counter action
  airplane-ding-dong.mp3            ← Default alert sound
  images/                           ← Plugin and button icons
  bin/
    win-x64/                        ← Windows release binary (not in source control)
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

## Twitch API Setup

The plugin communicates with the Twitch Helix API using an embedded Client ID and secret (XOR-obfuscated at compile time). OAuth authorisation is performed through the property inspector UI — click **Authorise with Twitch** and follow the browser prompt. The redirect lands on a local callback server on port **7878**.

Tokens are stored in the plugin's global settings via the StreamDock SDK and automatically refreshed when they expire.

---

## License

Private / proprietary — all rights reserved.


## Todo

1. Finish the "Following Channels Live" button.  This will allow the user to switch to a profile that has more keypad Icons.