use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::sync::Arc;
use tokio::sync::{broadcast, RwLock};
use chrono::Local;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LogEntry {
    pub timestamp: String,
    pub source: String,
    pub level: String, // info, success, error, debug
    pub message: String,
}

/// Helper to format current time
pub fn current_timestamp() -> String {
    Local::now().format("%Y-%m-%d %H:%M:%S").to_string()
}

/// Send log to Web UI (SSE) and store in memory
pub async fn log_to_web(
    log_tx: &broadcast::Sender<LogEntry>,
    recent_logs: &Arc<RwLock<VecDeque<LogEntry>>>,
    source: &str,
    level: &str,
    message: &str,
    limit: usize,
) {
    let entry = LogEntry {
        timestamp: current_timestamp(),
        source: source.to_string(),
        level: level.to_string(),
        message: message.to_string(),
    };

    // Broadcast to SSE clients
    let _ = log_tx.send(entry.clone());

    // Store in memory
    let mut logs = recent_logs.write().await;
    logs.push_front(entry);
    if logs.len() > limit {
        logs.pop_back();
    }
}
