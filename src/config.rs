use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;
use tokio::sync::RwLock;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Task {
    pub id: String,
    pub name: String,
    pub suffix: String,
    pub webhook_url: String,
    pub webhook_method: String,
    pub webhook_body: Option<String>,
    pub webhook_headers: HashMap<String, String>,
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default)]
    pub allow_api_trigger: bool,
}

fn default_true() -> bool {
    true
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AppConfig {
    pub log_limit: usize,
    #[serde(default)]
    pub run_on_startup: bool,
    pub tasks: Vec<Task>,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            log_limit: 100,
            run_on_startup: false,
            tasks: Vec::new(),
        }
    }
}

#[derive(Clone)]
pub struct ConfigManager {
    pub config: Arc<RwLock<AppConfig>>,
    file_path: String,
}

impl ConfigManager {
    pub async fn new(file_path: &str) -> Self {
        let config = if Path::new(file_path).exists() {
            let content = tokio::fs::read_to_string(file_path)
                .await
                .unwrap_or_default();
            serde_json::from_str(&content).unwrap_or_default()
        } else {
            AppConfig::default()
        };

        Self {
            config: Arc::new(RwLock::new(config)),
            file_path: file_path.to_string(),
        }
    }

    pub async fn save(&self) -> Result<()> {
        let config = self.config.read().await;
        let content = serde_json::to_string_pretty(&*config)?;
        tokio::fs::write(&self.file_path, content).await?;
        Ok(())
    }

    pub async fn update(&self, new_config: AppConfig) -> Result<()> {
        {
            let mut config = self.config.write().await;
            *config = new_config;
        }
        self.save().await
    }

    pub async fn get_tasks(&self) -> Vec<Task> {
        self.config.read().await.tasks.clone()
    }

    pub async fn get_log_limit(&self) -> usize {
        self.config.read().await.log_limit
    }

    pub async fn get_run_on_startup(&self) -> bool {
        self.config.read().await.run_on_startup
    }
}
