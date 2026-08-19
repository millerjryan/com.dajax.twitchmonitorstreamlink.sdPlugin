// Kick.com channel status via the public (unauthenticated) channel-info endpoint.
// No API keys, OAuth, or user login required — mirrors the public data shown on
// a channel's page, so monitoring Kick is exactly as plug-and-play as Twitch minus auth.

use reqwest::Client;
use serde::Deserialize;

// Kick fronts this endpoint with Cloudflare; a realistic desktop UA avoids
// being blocked as an obvious bot client.
const USER_AGENT: &str =
    "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/124.0 Safari/537.36";

#[derive(Deserialize)]
struct ChannelUser {
    username: String,
    #[serde(default)]
    profile_pic: Option<String>,
}

#[derive(Deserialize)]
struct Livestream {
    #[serde(default)]
    viewer_count: Option<u32>,
}

#[derive(Deserialize)]
struct ChannelResponse {
    user: ChannelUser,
    #[serde(default)]
    livestream: Option<Livestream>,
}

/// Live status snapshot for a Kick channel.
pub struct KickStatus {
    pub display_name: String,
    pub avatar_url:   Option<String>,
    pub is_live:      bool,
    pub viewer_count: Option<u32>,
}

fn http_client() -> Client {
    Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .build()
        .expect("HTTP client build failed")
}

/// Fetch live status + channel metadata for a Kick channel slug in one request.
pub async fn get_channel_status(slug: &str) -> Result<KickStatus, String> {
    let url = format!("https://kick.com/api/v2/channels/{}", slug.trim().to_lowercase());
    let resp = http_client()
        .get(&url)
        .header("User-Agent", USER_AGENT)
        .header("Accept", "application/json")
        .send()
        .await
        .map_err(|e| e.to_string())?;

    let status = resp.status();
    if status.as_u16() == 404 {
        return Err(format!("Channel not found: {slug}"));
    }
    if !status.is_success() {
        return Err(format!("Kick lookup failed: {status}"));
    }

    let body: ChannelResponse = resp.json().await.map_err(|e| e.to_string())?;
    let (is_live, viewer_count) = match body.livestream {
        Some(ls) => (true, ls.viewer_count),
        None     => (false, None),
    };

    Ok(KickStatus {
        display_name: body.user.username,
        avatar_url:   body.user.profile_pic,
        is_live,
        viewer_count,
    })
}
