use axum::{
    extract::{State, Json, Request},
    response::{sse::{Event, Sse}, IntoResponse, Response},
    routing::{get, post},
    middleware::{self, Next},
    Router,
    http::{Uri, header, StatusCode},
};
use std::sync::Arc;
use tokio::sync::broadcast;
use futures::stream::Stream;
use serde::{Deserialize, Serialize};
use crate::config::{ConfigManager, AppConfig, Task};
use crate::logging::{LogEntry, log_to_web};
use rust_embed::RustEmbed;
use std::net::Ipv6Addr;
use std::str::FromStr;
use colored::Colorize;
use chrono::Local;

#[derive(RustEmbed)]
#[folder = "static/"]
struct Assets;

use std::collections::VecDeque;
use tokio::sync::RwLock;

#[derive(Clone)]
pub struct AppState {
    pub config_manager: ConfigManager,
    pub log_tx: broadcast::Sender<LogEntry>,
    pub recent_logs: Arc<RwLock<VecDeque<LogEntry>>>,
}

pub async fn start_server(state: AppState, port: u16) {
    let app = Router::new()
        .route("/events", get(sse_handler))
        .route("/api/config", get(get_config).post(update_config))
        .route("/api/test-webhook", post(test_webhook))
        .route("/api/trigger/:task_name", post(trigger_task_handler))
        .fallback(static_handler)
        .layer(middleware::from_fn(access_log_middleware))
        .with_state(state);

    let listener = tokio::net::TcpListener::bind(format!("0.0.0.0:{}", port)).await.unwrap();
    println!("{} {} Web server listening on http://0.0.0.0:{}", Local::now().format("%Y-%m-%d %H:%M:%S"), "[Init]".green(), port);
    axum::serve(listener, app).await.unwrap();
}

async fn access_log_middleware(req: Request, next: Next) -> Response {
    let method = req.method().clone();
    let uri = req.uri().clone();
    let start = std::time::Instant::now();

    let response = next.run(req).await;

    let duration = start.elapsed();
    let status = response.status();
    
    let timestamp = Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
    let method_str = method.to_string();
    let method_colored = match method.as_str() {
        "GET" => method_str.green(),
        "POST" => method_str.yellow(),
        "PUT" => method_str.blue(),
        "PATCH" => method_str.magenta(),
        "DELETE" => method_str.red(),
        _ => method_str.normal(),
    };
    
    let status_str = status.as_u16().to_string();
    let status_colored = if status.is_success() {
        status_str.green()
    } else if status.is_server_error() {
        status_str.red()
    } else {
        status_str.yellow()
    };
    
    let tag = "[Web]".cyan();

    // Ignore SSE keepalive noise if needed, but useful to see connection
    if uri.path() != "/events" {
        println!("{} {} {} {} -> {} ({:?})", timestamp, tag, method_colored, uri, status_colored, duration);
    } else {
        // Log SSE connection establishment only
        println!("{} {} {} {} -> {} ({:?}) {}", timestamp, tag, method_colored, uri, status_colored, duration, "[SSE Connected]".magenta());
    }

    response
}

async fn static_handler(uri: Uri) -> impl IntoResponse {
    let mut path = uri.path().trim_start_matches('/').to_string();

    if path.is_empty() {
        path = "index.html".to_string();
    }

    match Assets::get(&path) {
        Some(content) => {
            let mime = mime_guess::from_path(&path).first_or_octet_stream();
            ([(header::CONTENT_TYPE, mime.as_ref())], content.data).into_response()
        }
        None => StatusCode::NOT_FOUND.into_response(),
    }
}

async fn sse_handler(
    State(state): State<AppState>,
) -> Sse<impl Stream<Item = Result<Event, axum::Error>>> {
    let mut rx = state.log_tx.subscribe();
    let recent_logs = state.recent_logs.read().await.clone();

    let stream = async_stream::stream! {
        // Send recent logs first, in reverse order (oldest to newest)
        // so that frontend can prepend them one by one to keep the order correct
        for log in recent_logs.iter().rev() {
             if let Ok(json) = serde_json::to_string(log) {
                yield Ok(Event::default().data(json));
            }
        }

        loop {
            match rx.recv().await {
                Ok(msg) => {
                    if let Ok(json) = serde_json::to_string(&msg) {
                        yield Ok(Event::default().data(json));
                    }
                }
                Err(broadcast::error::RecvError::Lagged(_)) => continue,
                Err(broadcast::error::RecvError::Closed) => break,
            }
        }
    };

    Sse::new(stream).keep_alive(axum::response::sse::KeepAlive::default())
}

async fn get_config(State(state): State<AppState>) -> Json<AppConfig> {
    let config = state.config_manager.config.read().await;
    Json(config.clone())
}

async fn update_config(
    State(state): State<AppState>,
    Json(new_config): Json<AppConfig>,
) -> impl IntoResponse {
    match state.config_manager.update(new_config).await {
        Ok(_) => "Config updated",
        Err(_) => "Failed to update config",
    }
}

#[derive(Deserialize)]
struct TestWebhookRequest {
    task: Task,
    fake_ip: String,
}

async fn test_webhook(
    State(_state): State<AppState>,
    Json(req): Json<TestWebhookRequest>,
) -> impl IntoResponse {
    let ip = match Ipv6Addr::from_str(&req.fake_ip) {
        Ok(ip) => ip,
        Err(_) => return "Invalid IPv6 address".to_string(),
    };

    // Simulate the logic (duplicate from main logic, should be refactored to shared function)
    let _ = u128::from_str_radix(&req.task.suffix.replace(":", ""), 16);
    let _suffix_u128 = 0; // Placeholder

    // Actually, let's use a helper function to combine IP.
    let combined = combine_ip(ip, &req.task.suffix);

    match combined {
        Ok(combined_ip) => {
             // Try sending the webhook (fire and forget or wait?)
             // For test, we wait.
             match send_webhook(&req.task, ip, combined_ip, Some(ip)).await {
                 Ok(status) => format!("Webhook sent! Status: {}", status),
                 Err(e) => format!("Webhook failed: {}", e),
             }
        }
        Err(e) => format!("Error combining IP: {}", e),
    }
}

pub fn combine_ip(original_ip: Ipv6Addr, suffix_str: &str) -> anyhow::Result<Ipv6Addr> {
    // suffix_str e.g. "::1" or "0:0:0:0:0:0:0:1"
    // If suffix starts with ::, it's relative?
    // User said: "addr & /64_mask | suffix"

    let original_u128 = u128::from(original_ip);
    let mask: u128 = !((1u128 << 64) - 1); // Top 64 bits are 1s
    let prefix = original_u128 & mask;

    // Parse suffix.
    // If user inputs "::1", Ipv6Addr::from_str("::1") gives 0...1.
    // If user inputs "1234::1", it gives 1234...1.
    // We strictly want the lower 64 bits from the suffix.
    let suffix_addr = Ipv6Addr::from_str(suffix_str)?;
    let suffix_u128 = u128::from(suffix_addr);

    let combined_u128 = prefix | (suffix_u128 & !mask);

    Ok(Ipv6Addr::from(combined_u128))
}

pub async fn send_webhook(task: &Task, original_ip: Ipv6Addr, combined_ip: Ipv6Addr, input_ip: Option<Ipv6Addr>) -> anyhow::Result<u16> {
    let client = reqwest::Client::builder()
        .user_agent(concat!("PrefixDDNS/", env!("CARGO_PKG_VERSION")))
        .build()?;

    let prefix_len = 64; // Hardcoded for now
    let prefix = format!("{}/{}", original_ip, prefix_len); // Roughly

    // Helper closure for replacement
    let do_replace = |text: &str| -> String {
        let mut s = text.to_string();
        s = s.replace("{{combined_ip}}", &combined_ip.to_string());
        s = s.replace("{{original_ip}}", &original_ip.to_string());
        if let Some(input) = input_ip {
            s = s.replace("{{input_ip}}", &input.to_string());
        }
        s = s.replace("{{prefix}}", &prefix);
        s
    };

    let url = do_replace(&task.webhook_url);
    let body = do_replace(&task.webhook_body.clone().unwrap_or_default());

    let mut req_builder = match task.webhook_method.to_uppercase().as_str() {
        "POST" => client.post(&url),
        "PUT" => client.put(&url),
        "PATCH" => client.patch(&url),
        _ => client.get(&url),
    };

    for (k, v) in &task.webhook_headers {
        req_builder = req_builder.header(k, v);
    }

    if !body.is_empty() {
        req_builder = req_builder.body(body);
    }

    let resp = req_builder.send().await?;
    let status = resp.status().as_u16();

    if !resp.status().is_success() {
        anyhow::bail!("HTTP {}", status);
    }

    Ok(status)
}

#[derive(Deserialize)]
struct TriggerRequest {
    ip: String,
}

#[derive(Serialize)]
struct ApiResponse<T> {
    status: String,
    message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    data: Option<T>,
}

impl<T> ApiResponse<T> {
    fn success(message: &str, data: Option<T>) -> Self {
        Self {
            status: "success".to_string(),
            message: message.to_string(),
            data,
        }
    }

    fn error(message: &str) -> Self {
        Self {
            status: "error".to_string(),
            message: message.to_string(),
            data: None,
        }
    }
}

async fn trigger_task_handler(
    State(state): State<AppState>,
    axum::extract::Path(task_name): axum::extract::Path<String>,
    Json(req): Json<TriggerRequest>,
) -> impl IntoResponse {
    let config = state.config_manager.config.read().await;
    let task = config.tasks.iter().find(|t| t.name == task_name);

    if let Some(task) = task {
        if !task.allow_api_trigger {
             return (axum::http::StatusCode::FORBIDDEN, Json(ApiResponse::<()>::error("API trigger disabled for this task"))).into_response();
        }

        let ip = match Ipv6Addr::from_str(&req.ip) {
            Ok(ip) => ip,
            Err(_) => return (axum::http::StatusCode::BAD_REQUEST, Json(ApiResponse::<()>::error("Invalid IPv6 address"))).into_response(),
        };

        let combined = match combine_ip(ip, &task.suffix) {
             Ok(c) => c,
             Err(e) => return (axum::http::StatusCode::INTERNAL_SERVER_ERROR, Json(ApiResponse::<()>::error(&format!("IP combination error: {}", e)))).into_response(),
        };

        let log_limit = config.log_limit;
        let log_msg = format!("Task [{}]: Running for {}", task.name, ip);
        log_to_web(&state.log_tx, &state.recent_logs, "API", "info", &log_msg, log_limit).await;

        match send_webhook(task, ip, combined, Some(ip)).await {
            Ok(status) => {
                 let success_msg = format!("Task [{}]: Success (HTTP {})", task.name, status);
                 log_to_web(&state.log_tx, &state.recent_logs, "API", "success", &success_msg, log_limit).await;
                 (axum::http::StatusCode::OK, Json(ApiResponse::<()>::success("Webhook triggered", None))).into_response()
            }
            Err(e) => {
                 let err_msg = format!("Task [{}]: Failed: {}", task.name, e);
                 log_to_web(&state.log_tx, &state.recent_logs, "API", "error", &err_msg, log_limit).await;
                 (axum::http::StatusCode::INTERNAL_SERVER_ERROR, Json(ApiResponse::<()>::error(&format!("Webhook failed: {}", e)))).into_response()
            }
        }
    } else {
        (axum::http::StatusCode::NOT_FOUND, Json(ApiResponse::<()>::error("Task not found"))).into_response()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    #[test]
    fn test_combine_ip_basic() {
        let prefix = Ipv6Addr::from_str("2001:db8:1:1::100").unwrap();
        let suffix = "::1";
        let combined = combine_ip(prefix, suffix).unwrap();
        assert_eq!(combined.to_string(), "2001:db8:1:1::1");
    }

    #[test]
    fn test_combine_ip_complex_suffix() {
        let prefix = Ipv6Addr::from_str("2001:db8:aaaa:bbbb::1").unwrap();
        let suffix = "::dead:beef";
        let combined = combine_ip(prefix, suffix).unwrap();
        assert_eq!(combined.to_string(), "2001:db8:aaaa:bbbb::dead:beef");
    }

    #[test]
    fn test_combine_ip_full_suffix() {
        let prefix = Ipv6Addr::from_str("2001:db8::1").unwrap();
        let suffix = "0:0:0:0:1:2:3:4";
        let combined = combine_ip(prefix, suffix).unwrap();
        assert_eq!(combined.to_string(), "2001:db8::1:2:3:4");
    }

    #[test]
    fn test_combine_ip_invalid_suffix() {
        let prefix = Ipv6Addr::from_str("2001:db8::1").unwrap();
        let suffix = "invalid";
        assert!(combine_ip(prefix, suffix).is_err());
    }
}
