use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::{broadcast, Notify, RwLock};
use tokio::time::Duration;

use crate::sse::SseEvent;
use crate::zt::client::ZtClient;
use crate::zt::models::ZtState;

const APP_NAME: &str = "tierdrop";
const CONFIG_FILENAME: &str = "config.json";

/// Returns the platform-appropriate data directory:
/// - Linux: ~/.local/share/tierdrop/
/// - Windows: %APPDATA%\tierdrop\
/// - macOS: ~/Library/Application Support/tierdrop/
fn data_dir() -> PathBuf {
    dirs::data_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(APP_NAME)
}

fn config_path() -> PathBuf {
    data_dir().join(CONFIG_FILENAME)
}

#[derive(Clone, serde::Serialize, serde::Deserialize)]
pub struct Config {
    pub username: String,
    pub password_hash: String,
    pub zt_token: String,
    #[serde(default = "default_zt_base_url")]
    pub zt_base_url: String,
    #[serde(default)]
    pub member_names: HashMap<String, String>,
}

fn default_zt_base_url() -> String {
    "http://localhost:9993".to_string()
}

impl Config {
    pub fn load() -> Option<Config> {
        let path = config_path();
        if !path.exists() {
            return None;
        }
        let data = std::fs::read_to_string(&path).ok()?;
        serde_json::from_str(&data).ok()
    }

    pub fn save(&self) -> Result<(), String> {
        let dir = data_dir();
        std::fs::create_dir_all(&dir)
            .map_err(|e| format!("Failed to create data dir {:?}: {}", dir, e))?;
        let path = config_path();
        let json = serde_json::to_string_pretty(self)
            .map_err(|e| format!("Failed to serialize config: {}", e))?;
        std::fs::write(&path, json)
            .map_err(|e| format!("Failed to write config {:?}: {}", path, e))?;
        Ok(())
    }
}

#[derive(Clone)]
pub struct AppState {
    pub zt_state: Arc<RwLock<ZtState>>,
    pub tx: broadcast::Sender<SseEvent>,
    pub config: Arc<RwLock<Option<Config>>>,
    pub zt_client: Arc<RwLock<Option<ZtClient>>>,
    pub poll_notify: Arc<Notify>,
}

impl AppState {
    pub fn new(config: Option<Config>) -> Self {
        let (tx, _rx) = broadcast::channel::<SseEvent>(64);
        Self {
            zt_state: Arc::new(RwLock::new(ZtState::default())),
            tx,
            config: Arc::new(RwLock::new(config)),
            zt_client: Arc::new(RwLock::new(None)),
            poll_notify: Arc::new(Notify::new()),
        }
    }

    /// Signal the poller to run immediately (e.g. after a mutation).
    pub fn notify_poller(&self) {
        self.poll_notify.notify_one();
    }

    pub async fn is_configured(&self) -> bool {
        self.config.read().await.is_some()
    }

    /// Initialize ZtClient from the stored config and start the background poller.
    pub async fn start_zt(&self) {
        let base_url;
        let zt_token;
        {
            let config = self.config.read().await;
            let config = match config.as_ref() {
                Some(c) => c,
                None => return,
            };
            base_url = std::env::var("ZT_BASE_URL")
                .unwrap_or_else(|_| config.zt_base_url.clone());
            zt_token = config.zt_token.clone();
        }

        let client = ZtClient::new(base_url, zt_token);
        {
            let mut w = self.zt_client.write().await;
            *w = Some(client.clone());
        }

        let poller_state = self.zt_state.clone();
        let poller_tx = self.tx.clone();
        let poller_notify = self.poll_notify.clone();
        tokio::spawn(async move {
            crate::zt::poller::start_poller(
                client,
                poller_state,
                poller_tx,
                poller_notify,
                Duration::from_secs(5),
            )
            .await;
        });
    }

    /// Save or remove a member display name. Empty name removes the entry.
    pub async fn save_member_name(&self, address: &str, name: &str) -> Result<(), String> {
        let mut cfg = self.config.write().await;
        if let Some(ref mut c) = *cfg {
            if name.is_empty() {
                c.member_names.remove(address);
            } else {
                c.member_names.insert(address.to_string(), name.to_string());
            }
            c.save()?;
        }
        Ok(())
    }

    /// Save config, update state, start ZT client + poller.
    pub async fn configure(&self, config: Config) -> Result<(), String> {
        config.save()?;
        {
            let mut w = self.config.write().await;
            *w = Some(config);
        }
        self.start_zt().await;
        Ok(())
    }
}
