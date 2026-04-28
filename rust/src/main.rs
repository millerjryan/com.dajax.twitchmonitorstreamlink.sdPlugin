mod twitch;
mod images;
mod audio;
mod oauth;

use std::collections::HashMap;
use std::env;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use tokio::sync::{Mutex, RwLock};
use tokio::time::interval;
use tokio_tungstenite::{connect_async, tungstenite::Message};

// ── Constants ──────────────────────────────────────────────────────────────

const PLUGIN_ACTION:  &str = "com.dajax.twitchmonitorstreamlink.monitor";
const FOLLOWS_ACTION: &str = "com.dajax.twitchmonitorstreamlink.followslive";
const POLL_INTERVAL:  Duration = Duration::from_secs(60);
const REDIRECT_PORT:  u16 = 7878;

// ── Credentials (XOR-obfuscated, same encoding as JS version) ─────────────

fn decode_cred(hex_csv: &str) -> String {
    let key = b"sdplug1n";
    hex_csv
        .split(',')
        .enumerate()
        .map(|(i, h)| {
            let byte = u8::from_str_radix(h.trim(), 16).unwrap_or(0);
            (byte ^ key[i % key.len()]) as char
        })
        .collect()
}

fn client_id() -> String {
    decode_cred("04,15,40,07,00,03,43,01,11,1d,47,0d,17,14,45,17,0b,52,19,04,14,0c,00,1d,06,1c,1f,0b,44,15")
}

fn client_secret() -> String {
    decode_cred("4b,51,40,04,1e,0b,57,19,45,54,13,0e,4c,0c,07,5f,40,0e,16,03,00,56,55,03,43,0a,47,5c,18,17")
}

// ── Shared state types ─────────────────────────────────────────────────────

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct GlobalSettings {
    #[serde(rename = "accessToken")]
    pub access_token: Option<String>,
    #[serde(rename = "refreshToken")]
    pub refresh_token: Option<String>,
    #[serde(rename = "tokenExpiry")]
    pub token_expiry: Option<u64>,
    #[serde(rename = "authedDisplayName")]
    pub authed_display_name: Option<String>,
    #[serde(rename = "authedAvatarUrl")]
    pub authed_avatar_url: Option<String>,
    #[serde(rename = "authedUserId")]
    pub authed_user_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ButtonSettings {
    #[serde(rename = "twitchUsername", default)]
    pub twitch_username: String,
    #[serde(rename = "alertEnabled", default)]
    pub alert_enabled: bool,
    #[serde(rename = "alertSoundData")]
    pub alert_sound_data: Option<String>,
    #[serde(rename = "alertSoundName", default)]
    pub alert_sound_name: String,
    #[serde(rename = "alertVolume", default = "default_volume")]
    pub alert_volume: u8,
    #[serde(rename = "buttonAction", default = "default_btn_action")]
    pub button_action: String,
    #[serde(rename = "streamlinkPath", default)]
    pub streamlink_path: String,
    #[serde(rename = "targetProfile", default)]
    pub target_profile: String,
}

fn default_volume() -> u8 { 80 }
fn default_btn_action() -> String { "browser".into() }

impl Default for ButtonSettings {
    fn default() -> Self {
        Self {
            twitch_username: String::new(),
            alert_enabled: false,
            alert_sound_data: None,
            alert_sound_name: String::new(),
            alert_volume: 80,
            button_action: "browser".into(),
            streamlink_path: String::new(),
            target_profile: String::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum ContextType { Monitor, Follows }

#[derive(Debug, Clone)]
pub struct ContextState {
    pub ctx_type:         ContextType,
    pub settings:         ButtonSettings,
    pub device:           Option<String>,
    // Monitor fields
    pub user_id:          Option<String>,
    pub avatar_url:       Option<String>,
    pub display_name:     Option<String>,
    pub is_live:          Option<bool>,
    pub viewer_count:     Option<u32>,
    pub has_image:        bool,
    // Follows fields
    pub follows_count:    Option<u32>,
}

impl ContextState {
    fn new_monitor(settings: ButtonSettings) -> Self {
        Self {
            ctx_type: ContextType::Monitor,
            settings,
            device: None,
            user_id: None,
            avatar_url: None,
            display_name: None,
            is_live: None,
            viewer_count: None,
            has_image: false,
            follows_count: None,
        }
    }
    fn new_follows(settings: ButtonSettings, device: Option<String>) -> Self {
        Self {
            ctx_type: ContextType::Follows,
            settings,
            device,
            user_id: None,
            avatar_url: None,
            display_name: None,
            is_live: None,
            viewer_count: None,
            has_image: false,
            follows_count: None,
        }
    }
}

// ── Shared app handle ──────────────────────────────────────────────────────

pub type WsSink = Arc<Mutex<futures_util::stream::SplitSink<
    tokio_tungstenite::WebSocketStream<
        tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>
    >,
    Message
>>>;

#[derive(Clone)]
pub struct AppHandle {
    pub sink:            WsSink,
    pub plugin_uuid:     String,
    pub global_settings: Arc<RwLock<GlobalSettings>>,
    pub contexts:        Arc<RwLock<HashMap<String, ContextState>>>,
    pub known_devices:   Arc<RwLock<Vec<String>>>,
}

impl AppHandle {
    pub async fn send(&self, obj: Value) {
        let text = obj.to_string();
        let _ = self.sink.lock().await.send(Message::Text(text.into())).await;
    }

    pub async fn set_image(&self, context: &str, b64: &str) {
        self.send(json!({
            "event": "setImage",
            "context": context,
            "payload": { "image": b64, "target": 0 }
        })).await;
    }

    pub async fn set_title(&self, context: &str, title: &str) {
        self.send(json!({
            "event": "setTitle",
            "context": context,
            "payload": { "title": title, "target": 0 }
        })).await;
    }

    pub async fn log(&self, msg: &str) {
        self.send(json!({
            "event": "logMessage",
            "payload": { "message": format!("[TwitchMonitor] {}", msg) }
        })).await;
    }

    pub async fn save_global_auth(&self, fields: GlobalSettings) {
        let mut gs = self.global_settings.write().await;
        if let Some(v) = fields.access_token   { gs.access_token   = Some(v); }
        if let Some(v) = fields.refresh_token  { gs.refresh_token  = Some(v); }
        if let Some(v) = fields.token_expiry   { gs.token_expiry   = Some(v); }
        if let Some(v) = fields.authed_display_name { gs.authed_display_name = Some(v); }
        if let Some(v) = fields.authed_avatar_url   { gs.authed_avatar_url   = Some(v); }
        if let Some(v) = fields.authed_user_id      { gs.authed_user_id      = Some(v); }
        let payload = serde_json::to_value(&*gs).unwrap_or_default();
        drop(gs);
        self.send(json!({
            "event":   "setGlobalSettings",
            "context": self.plugin_uuid,
            "payload": payload
        })).await;
    }

    pub async fn clear_global_auth(&self) {
        let mut gs = self.global_settings.write().await;
        gs.access_token        = None;
        gs.refresh_token       = None;
        gs.token_expiry        = None;
        gs.authed_display_name = None;
        gs.authed_avatar_url   = None;
        gs.authed_user_id      = None;
        let payload = serde_json::to_value(&*gs).unwrap_or_default();
        drop(gs);
        self.send(json!({
            "event":   "setGlobalSettings",
            "context": self.plugin_uuid,
            "payload": payload
        })).await;
    }

    pub async fn broadcast_auth_status(&self, payload: Value) {
        let ctxs: Vec<String> = self.contexts.read().await.keys().cloned().collect();
        for ctx in ctxs {
            self.send(json!({
                "action":  PLUGIN_ACTION,
                "event":   "sendToPropertyInspector",
                "context": ctx,
                "payload": payload
            })).await;
        }
    }
}

// ── Polling ────────────────────────────────────────────────────────────────

async fn poll_monitor(app: AppHandle, context: String) {
    let (token, username, prev_live, prev_viewer, user_id, avatar_url, display_name, has_image) = {
        let gs = app.global_settings.read().await;
        let ctxs = app.contexts.read().await;
        let state = match ctxs.get(&context) { Some(s) => s, None => return };
        (
            gs.access_token.clone(),
            state.settings.twitch_username.clone(),
            state.is_live,
            state.viewer_count,
            state.user_id.clone(),
            state.avatar_url.clone(),
            state.display_name.clone(),
            state.has_image,
        )
    };

    if token.is_none() {
        if let Ok(img) = images::placeholder(false).await {
            app.set_image(&context, &img).await;
        }
        app.set_title(&context, "Auth\nRequired").await;
        return;
    }

    if username.is_empty() {
        if let Ok(img) = images::placeholder(false).await {
            app.set_image(&context, &img).await;
        }
        app.set_title(&context, "Setup\nRequired").await;
        return;
    }

    // Proactive token refresh
    {
        let gs = app.global_settings.read().await;
        if let Some(expiry) = gs.token_expiry {
            let now_ms = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as u64;
            if now_ms > expiry.saturating_sub(5 * 60 * 1000) {
                drop(gs);
                if let Err(e) = twitch::refresh_token(app.clone()).await {
                    app.log(&format!("Proactive token refresh failed: {e}")).await;
                }
            }
        }
    }

    let token = app.global_settings.read().await.access_token.clone();
    let token = match token { Some(t) => t, None => return };

    // Resolve user ID on first poll
    let (user_id, avatar_url, display_name) = if user_id.is_none() {
        match twitch::get_user_info(&token, Some(&username.trim().to_lowercase())).await {
            Ok(info) => {
                let mut ctxs = app.contexts.write().await;
                if let Some(state) = ctxs.get_mut(&context) {
                    state.user_id      = Some(info.id.clone());
                    state.avatar_url   = Some(info.profile_image_url.clone());
                    state.display_name = Some(info.display_name.clone());
                }
                app.log(&format!("Resolved user: {} ({})", info.display_name, info.id)).await;
                (Some(info.id), Some(info.profile_image_url), Some(info.display_name))
            }
            Err(e) => {
                app.log(&format!("User lookup failed: {e}")).await;
                return;
            }
        }
    } else {
        (user_id, avatar_url, display_name)
    };

    let uid = match &user_id { Some(u) => u.clone(), None => return };

    let (is_live, viewer_count) = match twitch::get_stream_status(&token, &uid).await {
        Ok(s) => s,
        Err(e) => {
            let msg = e.to_string();
            app.log(&format!("Poll error: {msg}")).await;
            if msg.contains("401") {
                let app3 = app.clone();
                let ctx3 = context.clone();
                tokio::task::spawn_local(async move { handle_auth_expired(app3, ctx3).await; });
            }
            return;
        }
    };

    // Alert on offline → live transition
    if is_live && prev_live == Some(false) {
        let (alert_enabled, sound_data, volume) = {
            let ctxs = app.contexts.read().await;
            if let Some(state) = ctxs.get(&context) {
                let s = &state.settings;
                (s.alert_enabled, s.alert_sound_data.clone(), s.alert_volume)
            } else {
                (false, None, 80)
            }
        };
        if alert_enabled {
            if let Some(data) = sound_data {
                let vol = volume as f32 / 100.0;
                tokio::task::spawn_local(async move {
                    audio::play_base64_mp3(&data, vol).await;
                });
            }
        }
    }

    // Rebuild image when needed
    let viewer_changed = is_live && viewer_count != prev_viewer;
    if is_live != prev_live.unwrap_or(!is_live) || !has_image || viewer_changed {
        let img = match &avatar_url {
            Some(url) => match images::avatar(url, !is_live).await {
                Ok(i) => i,
                Err(_) => images::placeholder(is_live).await.unwrap_or_default(),
            },
            None => images::placeholder(is_live).await.unwrap_or_default(),
        };

        let img = if is_live {
            let img = images::add_live_badge(&img).await.unwrap_or(img);
            if let Some(vc) = viewer_count {
                images::add_viewer_count(&img, vc).await.unwrap_or(img)
            } else { img }
        } else { img };

        app.set_image(&context, &img).await;

        let mut ctxs = app.contexts.write().await;
        if let Some(state) = ctxs.get_mut(&context) {
            state.is_live      = Some(is_live);
            state.viewer_count = viewer_count;
            state.has_image    = true;
        }
    }

    app.set_title(&context, "").await;

    let dname = display_name.as_deref().unwrap_or(&username);
    app.send(json!({
        "action":  PLUGIN_ACTION,
        "event":   "sendToPropertyInspector",
        "context": context,
        "payload": {
            "displayName": dname,
            "avatarUrl":   avatar_url,
            "isLive":      is_live,
            "viewerCount": viewer_count,
        }
    })).await;
}

async fn poll_follows(app: AppHandle, context: String) {
    let (token, authed_user_id) = {
        let gs = app.global_settings.read().await;
        (gs.access_token.clone(), gs.authed_user_id.clone())
    };

    if token.is_none() {
        if let Ok(img) = images::placeholder(false).await {
            app.set_image(&context, &img).await;
        }
        app.set_title(&context, "Auth\nRequired").await;
        return;
    }
    let token = token.unwrap();

    // Resolve authed user ID if missing
    let uid = if let Some(id) = authed_user_id {
        id
    } else {
        match twitch::get_user_info(&token, None).await {
            Ok(info) => {
                app.save_global_auth(GlobalSettings {
                    authed_user_id:      Some(info.id.clone()),
                    authed_display_name: Some(info.display_name.clone()),
                    ..Default::default()
                }).await;
                info.id
            }
            Err(e) => {
                app.log(&format!("Could not resolve authed user ID: {e}")).await;
                app.set_title(&context, "Auth\nRequired").await;
                return;
            }
        }
    };

    // Proactive refresh
    {
        let gs = app.global_settings.read().await;
        if let Some(expiry) = gs.token_expiry {
            let now_ms = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as u64;
            if now_ms > expiry.saturating_sub(5 * 60 * 1000) {
                drop(gs);
                let _ = twitch::refresh_token(app.clone()).await;
            }
        }
    }

    let token = app.global_settings.read().await.access_token.clone().unwrap_or_default();

    match twitch::get_followed_live_count(&token, &uid).await {
        Ok(count) => {
            {
                let mut ctxs = app.contexts.write().await;
                if let Some(state) = ctxs.get_mut(&context) {
                    state.follows_count = Some(count);
                }
            }
            let img = images::follows_count_image(count).await.unwrap_or_default();
            app.set_image(&context, &img).await;
            app.set_title(&context, "").await;
            app.log(&format!("Follows live count: {count}")).await;
            app.send(json!({
                "action":  FOLLOWS_ACTION,
                "event":   "sendToPropertyInspector",
                "context": context,
                "payload": { "followsLiveCount": count }
            })).await;
        }
        Err(e) => {
            let msg = e.to_string();
            app.log(&format!("Follows poll error: {msg}")).await;
            if msg.contains("SCOPE_ERROR") {
                app.send(json!({
                    "action": FOLLOWS_ACTION,
                    "event": "sendToPropertyInspector",
                    "context": context,
                    "payload": {
                        "authStatus": "scope_error",
                        "authError": "Please reconnect your Twitch account to grant the follows permission."
                    }
                })).await;
                app.set_title(&context, "Re-Auth").await;
                return;
            }
            if msg.contains("401") {
                if let Ok(_) = twitch::refresh_token(app.clone()).await {
                    let t2 = app.global_settings.read().await.access_token.clone().unwrap_or_default();
                    if let Ok(count2) = twitch::get_followed_live_count(&t2, &uid).await {
                        let img = images::follows_count_image(count2).await.unwrap_or_default();
                        app.set_image(&context, &img).await;
                        app.set_title(&context, "").await;
                        return;
                    }
                }
                app.clear_global_auth().await;
                app.broadcast_auth_status(json!({
                    "authStatus": "expired",
                    "authError":  "Session expired — please reconnect your Twitch account."
                })).await;
            }
            app.set_title(&context, "Error").await;
        }
    }
}

async fn handle_auth_expired(app: AppHandle, context: String) {
    match twitch::refresh_token(app.clone()).await {
        Ok(_) => {
            let app2 = app.clone();
            let ctx = context.clone();
            tokio::task::spawn_local(async move { poll_monitor(app2, ctx).await; });
        }
        Err(e) => {
            app.log(&format!("Token refresh failed: {e}")).await;
            app.clear_global_auth().await;
            app.broadcast_auth_status(json!({
                "authStatus": "expired",
                "authError":  "Session expired — please reconnect your Twitch account."
            })).await;
            if let Ok(img) = images::placeholder(false).await {
                app.set_image(&context, &img).await;
            }
            app.set_title(&context, "Auth\nRequired").await;
        }
    }
}

// ── WebSocket message handler ──────────────────────────────────────────────

async fn handle_message(app: AppHandle, raw: &str) {
    let msg: Value = match serde_json::from_str(raw) {
        Ok(v) => v, Err(_) => return,
    };

    let event   = msg["event"].as_str().unwrap_or("");
    let action  = msg["action"].as_str().unwrap_or("");
    let context = msg["context"].as_str().unwrap_or("").to_string();
    let payload = &msg["payload"];

    match event {
        "deviceDidConnect" => {
            if let Some(dev) = msg["device"].as_str() {
                app.known_devices.write().await.push(dev.to_string());
                app.log(&format!("deviceDidConnect: {dev}")).await;
            }
        }

        "deviceDidDisconnect" => {
            if let Some(dev) = msg["device"].as_str() {
                app.known_devices.write().await.retain(|d| d != dev);
            }
        }

        "didReceiveGlobalSettings" => {
            let settings_val = &msg["payload"]["settings"];
            if let Ok(gs) = serde_json::from_value::<GlobalSettings>(settings_val.clone()) {
                *app.global_settings.write().await = gs;
            }
            let authed = app.global_settings.read().await.access_token.is_some();
            app.log(&format!("Global settings received — authed={authed}")).await;
            let ctxs: Vec<String> = app.contexts.read().await.keys().cloned().collect();
            for ctx in ctxs {
                let app2 = app.clone();
                let ctx2 = ctx.clone();
                tokio::task::spawn_local(async move { poll_context(app2, ctx2).await; });
            }
        }

        "willAppear" => {
            let settings: ButtonSettings = serde_json::from_value(
                payload.get("settings").cloned().unwrap_or(json!({}))
            ).unwrap_or_default();

            if action == PLUGIN_ACTION {
                app.log(&format!("willAppear (monitor): {context}")).await;
                app.contexts.write().await.insert(context.clone(), ContextState::new_monitor(settings));
                let app2 = app.clone();
                let ctx2 = context.clone();
                tokio::task::spawn_local(async move {
                    let mut timer = interval(POLL_INTERVAL);
                    timer.tick().await; // first tick fires immediately in tokio
                    loop {
                        poll_monitor(app2.clone(), ctx2.clone()).await;
                        timer.tick().await;
                        // Exit loop if context was removed
                        if !app2.contexts.read().await.contains_key(&ctx2) { break; }
                    }
                });
            } else if action == FOLLOWS_ACTION {
                app.log(&format!("willAppear (follows): {context}")).await;
                let device = msg["device"].as_str().map(str::to_string);
                app.contexts.write().await.insert(context.clone(), ContextState::new_follows(settings, device));
                let app2 = app.clone();
                let ctx2 = context.clone();
                tokio::task::spawn_local(async move {
                    let mut timer = interval(POLL_INTERVAL);
                    timer.tick().await;
                    loop {
                        poll_follows(app2.clone(), ctx2.clone()).await;
                        timer.tick().await;
                        if !app2.contexts.read().await.contains_key(&ctx2) { break; }
                    }
                });
            }
        }

        "willDisappear" => {
            if action == PLUGIN_ACTION || action == FOLLOWS_ACTION {
                app.log(&format!("willDisappear: {context}")).await;
                app.contexts.write().await.remove(&context);
            }
        }

        "didReceiveSettings" => {
            if action != PLUGIN_ACTION && action != FOLLOWS_ACTION { return; }
            let new_settings: ButtonSettings = serde_json::from_value(
                payload.get("settings").cloned().unwrap_or(json!({}))
            ).unwrap_or_default();
            app.log(&format!("didReceiveSettings: {context}")).await;

            let reset_user = {
                let ctxs = app.contexts.read().await;
                ctxs.get(&context).map(|s| {
                    s.settings.twitch_username != new_settings.twitch_username
                }).unwrap_or(false)
            };

            let mut ctxs = app.contexts.write().await;
            if let Some(state) = ctxs.get_mut(&context) {
                if reset_user && action == PLUGIN_ACTION {
                    state.user_id      = None;
                    state.avatar_url   = None;
                    state.display_name = None;
                    state.has_image    = false;
                    state.is_live      = None;
                    state.viewer_count = None;
                }
                state.settings = new_settings;
            }
            drop(ctxs);

            let app2 = app.clone();
            let ctx2 = context.clone();
            tokio::task::spawn_local(async move { poll_context(app2, ctx2).await; });
        }

        "keyDown" => {
            if action == FOLLOWS_ACTION {
                let ctxs = app.contexts.read().await;
                let state = ctxs.get(&context);
                let settings: ButtonSettings = serde_json::from_value(
                    payload.get("settings").cloned().unwrap_or(json!({}))
                ).unwrap_or_else(|_| state.map(|s| s.settings.clone()).unwrap_or_default());
                let profile = settings.target_profile.trim().to_string();

                let device = {
                    let d1 = state.and_then(|s| s.device.clone());
                    let d2 = msg["device"].as_str().map(str::to_string);
                    let d3 = app.known_devices.read().await.first().cloned();
                    d1.or(d2).or(d3)
                };
                drop(ctxs);

                if !profile.is_empty() {
                    let dev = device.clone();
                    app.log(&format!("keyDown(follows): profile={profile:?} device={dev:?}")).await;
                    let uuid = app.plugin_uuid.clone();
                    // Send both format variants (StreamDock compatibility)
                    app.send(json!({
                        "event": "switchToProfile",
                        "context": uuid,
                        "device": device,
                        "payload": { "profile": profile }
                    })).await;
                    app.send(json!({
                        "event": "switchToProfile",
                        "context": app.plugin_uuid,
                        "device": device,
                        "profile": profile
                    })).await;
                    app.send(json!({ "event": "showOk", "context": context })).await;
                } else {
                    app.send(json!({ "event": "showAlert", "context": context })).await;
                }
                return;
            }

            if action != PLUGIN_ACTION { return; }

            let (username, btn_action, streamlink_path) = {
                let ctxs = app.contexts.read().await;
                if let Some(state) = ctxs.get(&context) {
                    (
                        state.settings.twitch_username.clone(),
                        state.settings.button_action.clone(),
                        state.settings.streamlink_path.clone(),
                    )
                } else { return; }
            };

            if username.is_empty() { return; }

            if btn_action == "streamlink" {
                let exe = if streamlink_path.is_empty() { "streamlink".to_string() } else { streamlink_path };
                let url = format!("https://twitch.tv/{}", urlencoding::encode(&username));
                app.log(&format!("Launching streamlink: {exe} {url} best")).await;
                tokio::task::spawn_local(async move {
                    match std::process::Command::new(&exe)
                        .arg(&url)
                        .arg("best")
                        .spawn()
                    {
                        Ok(_) => {}
                        Err(e) => eprintln!("Failed to launch streamlink ({exe}): {e}"),
                    }
                });
            } else {
                let url = format!("https://twitch.tv/{}", urlencoding::encode(&username));
                app.send(json!({ "event": "openUrl", "payload": { "url": url } })).await;
            }
        }

        "sendToPlugin" => {
            if action != PLUGIN_ACTION && action != FOLLOWS_ACTION { return; }
            let cmd = payload["cmd"].as_str().unwrap_or("");
            match cmd {
                "startAuth" => {
                    if let Some(state_param) = payload["state"].as_str() {
                        app.log(&format!("Auth flow started for context {context}, state={state_param}")).await;
                    }
                }
                "cancelAuth" => {
                    app.log(&format!("Auth cancelled for context {context}")).await;
                }
                "clearAuth" => {
                    app.clear_global_auth().await;
                    let ctxs: Vec<String> = app.contexts.read().await.keys().cloned().collect();
                    for ctx in ctxs {
                        let app2 = app.clone();
                        let ctx2 = ctx.clone();
                        tokio::task::spawn_local(async move { poll_context(app2, ctx2).await; });
                    }
                    app.broadcast_auth_status(json!({ "authStatus": "cleared" })).await;
                }
                _ => {}
            }
        }

        _ => {}
    }
}

async fn poll_context(app: AppHandle, context: String) {
    let ctx_type = app.contexts.read().await.get(&context).map(|s| s.ctx_type.clone());
    match ctx_type {
        Some(ContextType::Monitor) => poll_monitor(app, context).await,
        Some(ContextType::Follows) => poll_follows(app, context).await,
        None => {}
    }
}

// ── Entry point ────────────────────────────────────────────────────────────

#[tokio::main(flavor = "current_thread")]
async fn main() {
    let local = tokio::task::LocalSet::new();
    local.run_until(async_main()).await;
}

async fn async_main() {
    let args: Vec<String> = env::args().collect();

    fn get_arg<'a>(args: &'a [String], flag: &str) -> Option<&'a str> {
        args.iter().position(|a| a == flag)
            .and_then(|i| args.get(i + 1))
            .map(String::as_str)
    }

    let port         = get_arg(&args, "-port").expect("Missing -port");
    let plugin_uuid  = get_arg(&args, "-pluginUUID").expect("Missing -pluginUUID");
    let register_evt = get_arg(&args, "-registerEvent").expect("Missing -registerEvent");

    // Seed known devices from -info JSON
    let known_devices: Vec<String> = get_arg(&args, "-info")
        .and_then(|s| serde_json::from_str::<Value>(s).ok())
        .and_then(|v| v["devices"].as_array().cloned())
        .map(|arr| arr.iter().filter_map(|d| d["id"].as_str().map(str::to_string)).collect())
        .unwrap_or_default();

    let ws_url = format!("ws://127.0.0.1:{port}");
    let (ws_stream, _) = connect_async(&ws_url).await.expect("WebSocket connection failed");
    let (sink, mut stream) = ws_stream.split();

    let app = AppHandle {
        sink:            Arc::new(Mutex::new(sink)),
        plugin_uuid:     plugin_uuid.to_string(),
        global_settings: Arc::new(RwLock::new(GlobalSettings::default())),
        contexts:        Arc::new(RwLock::new(HashMap::new())),
        known_devices:   Arc::new(RwLock::new(known_devices)),
    };

    // Register plugin and request global settings
    app.send(json!({ "event": register_evt, "uuid": plugin_uuid })).await;
    app.send(json!({ "event": "getGlobalSettings", "context": plugin_uuid })).await;
    app.log(&format!("Plugin connected — UUID={plugin_uuid}")).await;

    // Start OAuth callback HTTP server
    {
        let app2 = app.clone();
        let cid  = client_id();
        let csec = client_secret();
        tokio::task::spawn_local(async move {
            oauth::start_server(app2, cid, csec, REDIRECT_PORT).await;
        });
    }

    // WebSocket message loop
    while let Some(Ok(msg)) = stream.next().await {
        match msg {
            Message::Text(text) => {
                let app2 = app.clone();
                let t    = text.to_string();
                tokio::task::spawn_local(async move { handle_message(app2, &t).await; });
            }
            Message::Close(_) => {
                eprintln!("[TwitchMonitor] WebSocket closed");
                std::process::exit(0);
            }
            _ => {}
        }
    }
}

/// Simple percent-encoding for Twitch usernames in URLs.
mod urlencoding {
    pub fn encode(s: &str) -> String {
        s.chars().flat_map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.' | '~') {
                vec![c]
            } else {
                format!("%{:02X}", c as u32).chars().collect()
            }
        }).collect()
    }
}
