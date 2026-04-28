use std::net::SocketAddr;

use tokio::net::TcpListener;

use crate::AppHandle;

const SUCCESS_HTML: &str = r#"<html><head><meta charset="utf-8"><title>Twitch Auth</title>
<style>body{font-family:sans-serif;text-align:center;padding:60px;background:#1a1a2e;color:#e8e8f0}</style>
</head><body>
<h2 style="color:#00d4aa">&#10003; Connected to Twitch!</h2>
<p>You can close this tab and return to StreamDock.</p>
</body></html>"#;

const ERROR_HTML: &str = r#"<html><head><meta charset="utf-8"><title>Twitch Auth</title>
<style>body{font-family:sans-serif;text-align:center;padding:60px;background:#1a1a2e;color:#e8e8f0}</style>
</head><body>
<h2 style="color:#ff4757">&#10007; Authorization failed</h2>
<p>You can close this tab.</p>
</body></html>"#;

/// Start a persistent HTTP server on `port` to receive OAuth callbacks.
/// State format: `<nonce>.<percent-encoded-context>`
pub async fn start_server(app: AppHandle, _client_id: String, _client_secret: String, port: u16) {
    let addr = SocketAddr::from(([127, 0, 0, 1], port));
    let listener = match TcpListener::bind(addr).await {
        Ok(l) => l,
        Err(e) => {
            app.log(&format!("OAuth server bind error: {e} — retrying in 5s")).await;
            tokio::time::sleep(std::time::Duration::from_secs(5)).await;
            // Retry once
            match TcpListener::bind(addr).await {
                Ok(l) => l,
                Err(e2) => {
                    app.log(&format!("OAuth server failed to start: {e2}")).await;
                    return;
                }
            }
        }
    };

    app.log(&format!("OAuth callback server ready on port {port}")).await;

    loop {
        let (stream, _) = match listener.accept().await {
            Ok(s) => s, Err(_) => continue,
        };

        let app2 = app.clone();
        tokio::spawn(async move {
            handle_connection(app2, stream).await;
        });
    }
}

async fn handle_connection(app: AppHandle, mut stream: tokio::net::TcpStream) {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    let mut buf = [0u8; 4096];
    let n = match stream.read(&mut buf).await {
        Ok(n) if n > 0 => n,
        _ => return,
    };

    let request = String::from_utf8_lossy(&buf[..n]);
    let first_line = request.lines().next().unwrap_or("");

    // Parse "GET /path?query HTTP/1.1"
    let path_query = first_line
        .strip_prefix("GET ")
        .and_then(|s| s.split_whitespace().next())
        .unwrap_or("/");

    let (path, query) = match path_query.split_once('?') {
        Some((p, q)) => (p, q),
        None => (path_query, ""),
    };

    if path != "/" {
        let response = "HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\n\r\n";
        let _ = stream.write_all(response.as_bytes()).await;
        return;
    }

    // Parse query params
    let params: std::collections::HashMap<&str, &str> = query
        .split('&')
        .filter_map(|p| p.split_once('='))
        .collect();

    let code    = params.get("code").copied();
    let state   = params.get("state").copied().unwrap_or("");
    let error   = params.get("error").copied();

    // Decode context from state: "<nonce>.<percent-encoded-context>"
    let context = state.find('.').map(|i| {
        percent_decode(&state[i + 1..])
    });

    let (html, success) = if code.is_some() && context.is_some() && error.is_none() {
        (SUCCESS_HTML, true)
    } else {
        (ERROR_HTML, false)
    };

    let response = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        html.len(), html
    );
    let _ = stream.write_all(response.as_bytes()).await;

    if !success {
        app.log(&format!(
            "OAuth callback missing data — code={} context={} error={:?}",
            code.is_some(), context.is_some(), error
        )).await;
        if let Some(ctx) = &context {
            app.send(serde_json::json!({
                "action":  crate::PLUGIN_ACTION,
                "event":   "sendToPropertyInspector",
                "context": ctx,
                "payload": {
                    "authStatus": "error",
                    "authError":  error.unwrap_or("Authorization denied")
                }
            })).await;
        }
        return;
    }

    let code    = code.unwrap();
    let context = context.unwrap();
    let redirect_uri = format!("http://localhost:{}", crate::REDIRECT_PORT);

    match crate::twitch::exchange_code(app.clone(), code, &context, &redirect_uri).await {
        Ok(_) => {}
        Err(e) => {
            app.log(&format!("OAuth callback error: {e}")).await;
            app.send(serde_json::json!({
                "action":  crate::PLUGIN_ACTION,
                "event":   "sendToPropertyInspector",
                "context": context,
                "payload": { "authStatus": "error", "authError": e }
            })).await;
        }
    }
}

fn percent_decode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let Ok(hex) = std::str::from_utf8(&bytes[i + 1..i + 3]) {
                if let Ok(byte) = u8::from_str_radix(hex, 16) {
                    out.push(byte as char);
                    i += 3;
                    continue;
                }
            }
        } else if bytes[i] == b'+' {
            out.push(' ');
            i += 1;
            continue;
        }
        out.push(bytes[i] as char);
        i += 1;
    }
    out
}
