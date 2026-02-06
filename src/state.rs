use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use chrono::{DateTime, Utc};
use tokio::sync::{broadcast, Notify, RwLock};
use tokio::time::Duration;

use crate::sse::SseEvent;
use crate::zt::client::ZtClient;
use crate::zt::models::ZtState;

const APP_NAME: &str = "tierdrop";
const CONFIG_FILENAME: &str = "config.json";

/// Per-network permissions for a user
#[derive(Clone, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct NetworkPermissions {
    pub read: bool,      // Can view network and members
    pub authorize: bool, // Can authorize/deauthorize members
    pub modify: bool,    // Can edit network settings, IP pools, routes
    pub delete: bool,    // Can delete network or remove members
}

impl NetworkPermissions {
    /// Full permissions (all true)
    pub fn full() -> Self {
        Self {
            read: true,
            authorize: true,
            modify: true,
            delete: true,
        }
    }

    /// Check if user has any permission on this network
    pub fn has_any(&self) -> bool {
        self.read || self.authorize || self.modify || self.delete
    }
}

/// A user account
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct User {
    pub id: u64,
    pub username: String,
    pub password_hash: String,
    pub is_admin: bool,
    #[serde(default)]
    pub network_permissions: HashMap<String, NetworkPermissions>,
    pub created_at: DateTime<Utc>,
}

impl User {
    /// Create a new admin user with specified ID
    pub fn new_admin(id: u64, username: String, password_hash: String) -> Self {
        Self {
            id,
            username,
            password_hash,
            is_admin: true,
            network_permissions: HashMap::new(),
            created_at: Utc::now(),
        }
    }

    /// Create a new regular user with specified ID
    pub fn new(id: u64, username: String, password_hash: String, is_admin: bool) -> Self {
        Self {
            id,
            username,
            password_hash,
            is_admin,
            network_permissions: HashMap::new(),
            created_at: Utc::now(),
        }
    }

    /// Get permissions for a specific network
    pub fn get_network_permissions(&self, nwid: &str) -> NetworkPermissions {
        if self.is_admin {
            NetworkPermissions::full()
        } else {
            self.network_permissions.get(nwid).cloned().unwrap_or_default()
        }
    }

    /// Check if user can access any network (for dashboard visibility)
    pub fn can_access_any_network(&self) -> bool {
        if self.is_admin {
            return true;
        }
        self.network_permissions.values().any(|p| p.has_any())
    }

    /// Count networks user has access to
    pub fn accessible_network_count(&self) -> usize {
        if self.is_admin {
            return usize::MAX; // Shown as "All" in UI
        }
        self.network_permissions.values().filter(|p| p.has_any()).count()
    }
}

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
    // Legacy fields (kept for backwards compatibility during migration)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub username: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub password_hash: Option<String>,

    // New multi-user support
    #[serde(default)]
    pub users: Vec<User>,
    #[serde(default = "default_next_user_id")]
    pub next_user_id: u64,

    pub zt_token: String,
    #[serde(default = "default_zt_base_url")]
    pub zt_base_url: String,
    #[serde(default)]
    pub member_names: HashMap<String, String>,
    #[serde(default)]
    pub rules_source: HashMap<String, String>,  // nwid -> DSL source
}

fn default_next_user_id() -> u64 {
    1
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
        let mut config: Config = serde_json::from_str(&data).ok()?;

        // Migration: if old username/password_hash exist but no users, create admin
        if config.users.is_empty() {
            if let (Some(username), Some(password_hash)) = (&config.username, &config.password_hash) {
                let admin = User::new_admin(config.next_user_id, username.clone(), password_hash.clone());
                config.next_user_id += 1;
                config.users.push(admin);
                // Clear legacy fields
                config.username = None;
                config.password_hash = None;
                // Save migrated config
                let _ = config.save();
            }
        }

        // Migration: ensure next_user_id is greater than all existing user IDs
        if let Some(max_id) = config.users.iter().map(|u| u.id).max() {
            if config.next_user_id <= max_id {
                config.next_user_id = max_id + 1;
                let _ = config.save();
            }
        }

        Some(config)
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

    /// Find a user by username
    pub fn find_user_by_username(&self, username: &str) -> Option<&User> {
        self.users.iter().find(|u| u.username == username)
    }

    /// Find a user by ID
    pub fn find_user_by_id(&self, id: u64) -> Option<&User> {
        self.users.iter().find(|u| u.id == id)
    }

    /// Find a user by ID (mutable)
    pub fn find_user_by_id_mut(&mut self, id: u64) -> Option<&mut User> {
        self.users.iter_mut().find(|u| u.id == id)
    }

    /// Add a new user with auto-generated ID
    pub fn add_user(&mut self, username: String, password_hash: String, is_admin: bool) -> &User {
        let user = User::new(self.next_user_id, username, password_hash, is_admin);
        self.next_user_id += 1;
        self.users.push(user);
        self.users.last().unwrap()
    }

    /// Remove a user by ID (returns true if removed)
    pub fn remove_user(&mut self, id: u64) -> bool {
        let len_before = self.users.len();
        self.users.retain(|u| u.id != id);
        self.users.len() < len_before
    }

    /// Check if there's at least one admin user
    pub fn has_admin(&self) -> bool {
        self.users.iter().any(|u| u.is_admin)
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

    /// Save or remove flow rules source DSL for a network. Empty source removes the entry.
    pub async fn save_rules_source(&self, nwid: &str, source: &str) -> Result<(), String> {
        let mut cfg = self.config.write().await;
        if let Some(ref mut c) = *cfg {
            if source.is_empty() {
                c.rules_source.remove(nwid);
            } else {
                c.rules_source.insert(nwid.to_string(), source.to_string());
            }
            c.save()?;
        }
        Ok(())
    }

    /// Get the stored flow rules source DSL for a network.
    pub async fn _get_rules_source(&self, nwid: &str) -> Option<String> {
        let cfg = self.config.read().await;
        cfg.as_ref().and_then(|c| c.rules_source.get(nwid).cloned())
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
