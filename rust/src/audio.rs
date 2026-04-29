use std::io::Cursor;
use base64::{engine::general_purpose::STANDARD as B64, Engine};
use rodio::{Decoder, OutputStream, Sink};

/// Return the path to the bundled `airplane-ding-dong.mp3` that ships with the plugin.
/// The binary lives at `…/bin/<platform>/twitchmonitor-plugin(.exe)`;
/// the MP3 is three directories up at the plugin root.
pub fn find_bundled_sound() -> Option<std::path::PathBuf> {
    let exe  = std::env::current_exe().ok()?;
    let root = exe.parent()?.parent()?.parent()?;
    let mp3  = root.join("airplane-ding-dong.mp3");
    if mp3.exists() { Some(mp3) } else { None }
}

/// Decode and play raw MP3 bytes at the given volume (0.0–1.0).
/// Runs in a blocking thread so the tokio runtime is not stalled.
pub async fn play_bytes(bytes: Vec<u8>, volume: f32) {
    tokio::task::spawn_blocking(move || {
        let (_stream, handle) = match OutputStream::try_default() {
            Ok(v)  => v,
            Err(e) => { eprintln!("[audio] no output device: {e}"); return; }
        };
        let sink = match Sink::try_new(&handle) {
            Ok(s)  => s,
            Err(e) => { eprintln!("[audio] sink error: {e}"); return; }
        };
        sink.set_volume(volume.clamp(0.0, 1.0));
        let source = match Decoder::new(Cursor::new(bytes)) {
            Ok(d)  => d,
            Err(e) => { eprintln!("[audio] decode error: {e}"); return; }
        };
        sink.append(source);
        sink.sleep_until_end();
    }).await.ok();
}

/// Play a local MP3 file.
pub async fn play_file(path: &std::path::Path, volume: f32) {
    match std::fs::read(path) {
        Ok(bytes) => play_bytes(bytes, volume).await,
        Err(e)    => eprintln!("[audio] read {}: {e}", path.display()),
    }
}

/// Play an MP3 supplied as a base64 data URI. `volume` is 0.0–1.0.
pub async fn play_base64_mp3(data_uri: &str, volume: f32) {
    // Strip the data URI header — everything after the first comma is base64.
    let base64_part = match data_uri.find(',') {
        Some(idx) => &data_uri[idx + 1..],
        None      => data_uri,
    };
    match B64.decode(base64_part) {
        Ok(bytes) => play_bytes(bytes, volume).await,
        Err(e)    => eprintln!("[audio] base64 decode error: {e}"),
    }
}
