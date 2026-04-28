use std::time::{SystemTime, UNIX_EPOCH};

use reqwest::Client;
use serde::Deserialize;

use crate::{AppHandle, GlobalSettings};

// ── Response types ─────────────────────────────────────────────────────────

#[derive(Deserialize)]
pub struct TwitchUser {
    pub id:                String,
    pub display_name:      String,
    pub profile_image_url: String,
}

#[derive(Deserialize)]
struct UsersResponse {
    data: Vec<TwitchUser>,
}

#[derive(Deserialize)]
struct StreamsResponse {
    data: Vec<StreamEntry>,
    #[serde(default)]
    pagination: Pagination,
}

#[derive(Deserialize, Default)]
struct Pagination {
    cursor: Option<String>,
}

#[derive(Deserialize)]
struct StreamEntry {
    viewer_count: u32,
}

#[derive(Deserialize)]
struct TokenResponse {
    access_token:  String,
    refresh_token: Option<String>,
    expires_in:    u64,
}

// ── Helpers ────────────────────────────────────────────────────────────────

fn http_client() -> Client {
    Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .build()
        .expect("HTTP client build failed")
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

// ── Public API ─────────────────────────────────────────────────────────────

/// Look up a Twitch user by login name, or the authenticated user if `login` is None.
pub async fn get_user_info(token: &str, login: Option<&str>) -> Result<TwitchUser, String> {
    let cid = crate::client_id();
    let url = match login {
        Some(l) => format!("https://api.twitch.tv/helix/users?login={}", l),
        None    => "https://api.twitch.tv/helix/users".to_string(),
    };
    let resp = http_client()
        .get(&url)
        .header("Client-ID", &cid)
        .header("Authorization", format!("Bearer {}", token))
        .send()
        .await
        .map_err(|e| e.to_string())?;

    if !resp.status().is_success() {
        return Err(format!("User lookup failed: {}", resp.status()));
    }
    let body: UsersResponse = resp.json().await.map_err(|e| e.to_string())?;
    body.data.into_iter().next().ok_or_else(|| {
        format!("User not found: {}", login.unwrap_or("(authed user)"))
    })
}

/// Return (is_live, viewer_count) for a user ID.
pub async fn get_stream_status(token: &str, user_id: &str) -> Result<(bool, Option<u32>), String> {
    let cid  = crate::client_id();
    let url  = format!("https://api.twitch.tv/helix/streams?user_id={}", user_id);
    let resp = http_client()
        .get(&url)
        .header("Client-ID", &cid)
        .header("Authorization", format!("Bearer {}", token))
        .send()
        .await
        .map_err(|e| e.to_string())?;

    if !resp.status().is_success() {
        return Err(format!("Stream lookup failed: {}", resp.status()));
    }
    let body: StreamsResponse = resp.json().await.map_err(|e| e.to_string())?;
    match body.data.into_iter().next() {
        Some(s) => Ok((true,  Some(s.viewer_count))),
        None    => Ok((false, None)),
    }
}

/// Count how many channels the authed user follows that are currently live.
/// Paginates up to 500 results.
pub async fn get_followed_live_count(token: &str, user_id: &str) -> Result<u32, String> {
    let cid    = crate::client_id();
    let client = http_client();
    let mut total: u32 = 0;
    let mut cursor: Option<String> = None;

    loop {
        let mut url = format!(
            "https://api.twitch.tv/helix/streams/followed?user_id={}&first=100",
            user_id
        );
        if let Some(c) = &cursor {
            url.push_str(&format!("&after={}", c));
        }

        let resp = client
            .get(&url)
            .header("Client-ID", &cid)
            .header("Authorization", format!("Bearer {}", token))
            .send()
            .await
            .map_err(|e| e.to_string())?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body_text = resp.text().await.unwrap_or_default().to_lowercase();
            if status.as_u16() == 401 && body_text.contains("scope") {
                return Err("SCOPE_ERROR: user:read:follows scope required — please reconnect".to_string());
            }
            return Err(format!("Followed streams lookup failed: {status}"));
        }

        let body: StreamsResponse = resp.json().await.map_err(|e| e.to_string())?;
        total += body.data.len() as u32;
        cursor = body.pagination.cursor;

        if cursor.is_none() || total >= 500 { break; }
    }
    Ok(total)
}

/// Refresh the OAuth access token stored in GlobalSettings.
pub async fn refresh_token(app: AppHandle) -> Result<String, String> {
    let refresh = app.global_settings.read().await.refresh_token.clone()
        .ok_or("No refresh token — user must re-authenticate")?;

    let cid  = crate::client_id();
    let csec = crate::client_secret();

    let params = [
        ("grant_type",    "refresh_token"),
        ("refresh_token", &refresh),
        ("client_id",     &cid),
        ("client_secret", &csec),
    ];

    let resp = http_client()
        .post("https://id.twitch.tv/oauth2/token")
        .form(&params)
        .send()
        .await
        .map_err(|e| e.to_string())?;

    if !resp.status().is_success() {
        return Err(format!("Token refresh failed: {}", resp.status()));
    }

    let tok: TokenResponse = resp.json().await.map_err(|e| e.to_string())?;
    let new_refresh = tok.refresh_token.clone()
        .unwrap_or(refresh);

    app.save_global_auth(GlobalSettings {
        access_token:  Some(tok.access_token.clone()),
        refresh_token: Some(new_refresh),
        token_expiry:  Some(now_ms() + tok.expires_in * 1000),
        ..Default::default()
    }).await;

    app.log("Token refreshed (global)").await;
    Ok(tok.access_token)
}

/// Exchange an OAuth authorization code for access + refresh tokens, then persist them.
pub async fn exchange_code(
    app:          AppHandle,
    code:         &str,
    context:      &str,
    redirect_uri: &str,
) -> Result<(), String> {
    let cid  = crate::client_id();
    let csec = crate::client_secret();

    let params = [
        ("client_id",     cid.as_str()),
        ("client_secret", csec.as_str()),
        ("code",          code),
        ("grant_type",    "authorization_code"),
        ("redirect_uri",  redirect_uri),
    ];

    let resp = http_client()
        .post("https://id.twitch.tv/oauth2/token")
        .form(&params)
        .send()
        .await
        .map_err(|e| e.to_string())?;

    if !resp.status().is_success() {
        return Err(format!("Token exchange failed: {}", resp.status()));
    }

    let tok: TokenResponse = resp.json().await.map_err(|e| e.to_string())?;
    let user_info = get_user_info(&tok.access_token, None).await?;

    app.save_global_auth(GlobalSettings {
        access_token:        Some(tok.access_token.clone()),
        refresh_token:       tok.refresh_token,
        token_expiry:        Some(now_ms() + tok.expires_in * 1000),
        authed_display_name: Some(user_info.display_name.clone()),
        authed_avatar_url:   Some(user_info.profile_image_url.clone()),
        authed_user_id:      Some(user_info.id.clone()),
    }).await;

    // Restart polls for all contexts
    {
        let ctxs: Vec<String> = app.contexts.read().await.keys().cloned().collect();
        for ctx in ctxs {
            let app2 = app.clone();
            let ctx2 = ctx.clone();
            tokio::spawn(async move { crate::poll_context(app2, ctx2).await; });
        }
    }

    app.broadcast_auth_status(serde_json::json!({
        "authStatus":        "success",
        "authedDisplayName": user_info.display_name,
        "authedAvatarUrl":   user_info.profile_image_url,
    })).await;

    app.log(&format!("OAuth success — authenticated as {}", user_info.display_name)).await;
    Ok(())
}
