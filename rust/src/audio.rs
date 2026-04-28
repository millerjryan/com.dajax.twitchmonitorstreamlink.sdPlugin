use std::time::{SystemTime, UNIX_EPOCH};
use tokio::process::Command;
use base64::{engine::general_purpose::STANDARD as B64, Engine};

/// Play an MP3 supplied as a base64 data URI.
/// `volume` is 0.0 – 1.0.
pub async fn play_base64_mp3(data_uri: &str, volume: f32) {
    let base64_part = data_uri.trim_start_matches("data:audio/mpeg;base64,")
                              .trim_start_matches("data:audio/mp3;base64,")
                              .trim_start_matches("data:audio/mpeg3;base64,");

    let audio_bytes = match B64.decode(base64_part) {
        Ok(b) => b,
        Err(_) => return,
    };

    // Write to temp file
    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    let tmp_path = std::env::temp_dir().join(format!("twitch_alert_{ts}.mp3"));

    if tokio::fs::write(&tmp_path, &audio_bytes).await.is_err() {
        return;
    }

    // Resolve to long path on Windows to avoid 8.3 short names
    let resolved = tokio::fs::canonicalize(&tmp_path).await
        .unwrap_or_else(|_| tmp_path.clone());

    let vol_pct = (volume.clamp(0.0, 1.0) * 100.0).round() as u32;

    #[cfg(target_os = "windows")]
    {
        let path_str = resolved.to_string_lossy().replace('\'', "''");
        let ps_cmd = format!(
            "$wmp = New-Object -ComObject WMPlayer.OCX; \
             $wmp.settings.volume = {vol_pct}; \
             $wmp.URL = '{path_str}'; \
             $wmp.controls.play(); \
             Start-Sleep -Milliseconds 500; \
             $t = 0; \
             while (($wmp.playState -eq 3 -or $wmp.playState -eq 6 -or $wmp.playState -eq 9) -and $t -lt 120) \
             {{ Start-Sleep -Milliseconds 500; $t++ }}; \
             $wmp.close()"
        );
        let _ = Command::new("powershell.exe")
            .args(["-NoProfile", "-NonInteractive", "-WindowStyle", "Hidden", "-Command", &ps_cmd])
            .spawn();
    }

    #[cfg(target_os = "macos")]
    {
        let af_vol = (volume * 2.55) as f64;
        let path_str = resolved.to_string_lossy().to_string();
        let _ = Command::new("afplay")
            .args(["-v", &format!("{:.2}", af_vol), &path_str])
            .spawn();
    }

    // Cleanup handled by OS temp management; we don't block on playback completion here.
    // The WMPlayer command blocks until playback finishes, so the temp file is
    // safe to leave for OS cleanup on subsequent boot.
}
