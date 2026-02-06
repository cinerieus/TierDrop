use axum::body::Body;
use axum::extract::State;
use axum::http::header::{CONTENT_DISPOSITION, CONTENT_TYPE};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Extension;
use axum_extra::extract::Multipart;
use chrono::{DateTime, Utc};
use flate2::read::GzDecoder;
use flate2::write::GzEncoder;
use flate2::Compression;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use tar::{Archive, Builder};
use tempfile::TempDir;

use crate::permissions;
use crate::state::{AppState, User};
use crate::zt::client::ZtClient;

#[derive(Debug, Serialize, Deserialize)]
pub struct Manifest {
    pub version: u32,
    pub created_at: DateTime<Utc>,
    pub node_address: Option<String>,
    pub backup_type: String,
    pub network_count: usize,
    pub tierdrop_version: String,
}

/// Returns the platform-appropriate ZeroTier data directory
fn zerotier_data_dir() -> PathBuf {
    #[cfg(target_os = "windows")]
    {
        PathBuf::from(r"C:\ProgramData\ZeroTier\One")
    }
    #[cfg(target_os = "macos")]
    {
        PathBuf::from("/Library/Application Support/ZeroTier/One")
    }
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    {
        PathBuf::from("/var/lib/zerotier-one")
    }
}

/// Recursively copy a directory
fn copy_dir_recursive(src: &Path, dst: &Path) -> std::io::Result<()> {
    if !src.exists() {
        return Ok(());
    }
    std::fs::create_dir_all(dst)?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let path = entry.path();
        let dest_path = dst.join(entry.file_name());
        if path.is_dir() {
            copy_dir_recursive(&path, &dest_path)?;
        } else {
            std::fs::copy(&path, &dest_path)?;
        }
    }
    Ok(())
}

/// Count networks in the backup (from controller.d directory)
fn count_networks(temp_dir: &Path) -> usize {
    let controller_d = temp_dir.join("zerotier-one").join("controller.d");
    if !controller_d.exists() {
        return 0;
    }
    std::fs::read_dir(&controller_d)
        .map(|entries| {
            entries
                .filter_map(|e| e.ok())
                .filter(|e| {
                    e.path().extension().map(|ext| ext == "json").unwrap_or(false)
                })
                .count()
        })
        .unwrap_or(0)
}

/// Determine backup type based on presence of identity files
fn determine_backup_type(temp_dir: &Path) -> &'static str {
    let zt_path = temp_dir.join("zerotier-one");
    if zt_path.join("identity.secret").exists() && zt_path.join("identity.public").exists() {
        "full"
    } else {
        "partial"
    }
}

/// Create a tar.gz archive from a directory
fn create_tar_gz(source_dir: &Path, archive_name: &str) -> std::io::Result<Vec<u8>> {
    let mut buffer = Vec::new();
    {
        let encoder = GzEncoder::new(&mut buffer, Compression::default());
        let mut tar = Builder::new(encoder);

        // Add all files from the temp directory under the archive name prefix
        for entry in std::fs::read_dir(source_dir)? {
            let entry = entry?;
            let path = entry.path();
            let name = format!("{}/{}", archive_name, entry.file_name().to_string_lossy());

            if path.is_dir() {
                tar.append_dir_all(&name, &path)?;
            } else {
                tar.append_path_with_name(&path, &name)?;
            }
        }

        tar.into_inner()?.finish()?;
    }
    Ok(buffer)
}

/// Extract a tar.gz archive to a temporary directory
fn extract_tar_gz(data: &[u8]) -> std::io::Result<TempDir> {
    let temp_dir = tempfile::tempdir()?;
    let decoder = GzDecoder::new(data);
    let mut archive = Archive::new(decoder);

    // Extract to temp directory
    archive.unpack(temp_dir.path())?;

    Ok(temp_dir)
}

/// Export backup handler - creates and downloads a tar.gz backup
pub async fn export_backup(
    State(state): State<AppState>,
    Extension(user): Extension<User>,
) -> Response {
    // Only admins can export backups
    if !permissions::is_admin(&user) {
        return (StatusCode::FORBIDDEN, "Only administrators can export backups").into_response();
    }

    // Create temp directory for staging
    let temp_dir = match tempfile::tempdir() {
        Ok(d) => d,
        Err(e) => {
            return (
                axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to create temp directory: {}", e),
            )
                .into_response();
        }
    };

    // Copy ZeroTier directory
    let zt_dir = zerotier_data_dir();
    let zt_dest = temp_dir.path().join("zerotier-one");
    if let Err(e) = copy_dir_recursive(&zt_dir, &zt_dest) {
        tracing::warn!("Failed to copy ZeroTier directory: {}", e);
        // Continue anyway - might be permission issues
    }

    // Copy TierDrop config
    {
        let config = state.config.read().await;
        if let Some(ref c) = *config {
            let config_json = match serde_json::to_string_pretty(c) {
                Ok(j) => j,
                Err(e) => {
                    return (
                        axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                        format!("Failed to serialize config: {}", e),
                    )
                        .into_response();
                }
            };
            if let Err(e) = std::fs::write(temp_dir.path().join("tierdrop-config.json"), config_json)
            {
                return (
                    axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                    format!("Failed to write config: {}", e),
                )
                    .into_response();
            }
        }
    }

    // Create manifest
    let node_address = {
        let zt = state.zt_state.read().await;
        zt.status.as_ref().and_then(|s| s.address.clone())
    };

    let manifest = Manifest {
        version: 1,
        created_at: Utc::now(),
        node_address,
        backup_type: determine_backup_type(temp_dir.path()).to_string(),
        network_count: count_networks(temp_dir.path()),
        tierdrop_version: env!("CARGO_PKG_VERSION").to_string(),
    };

    let manifest_json = match serde_json::to_string_pretty(&manifest) {
        Ok(j) => j,
        Err(e) => {
            return (
                axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to serialize manifest: {}", e),
            )
                .into_response();
        }
    };
    if let Err(e) = std::fs::write(temp_dir.path().join("manifest.json"), manifest_json) {
        return (
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to write manifest: {}", e),
        )
            .into_response();
    }

    // Create archive
    let timestamp = Utc::now().format("%Y%m%d-%H%M%S");
    let archive_name = format!("tierdrop-backup-{}", timestamp);
    let archive_data = match create_tar_gz(temp_dir.path(), &archive_name) {
        Ok(d) => d,
        Err(e) => {
            return (
                axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to create archive: {}", e),
            )
                .into_response();
        }
    };

    let filename = format!("{}.tar.gz", archive_name);

    Response::builder()
        .header(CONTENT_TYPE, "application/gzip")
        .header(
            CONTENT_DISPOSITION,
            format!("attachment; filename=\"{}\"", filename),
        )
        .body(Body::from(archive_data))
        .unwrap()
}

#[derive(Debug)]
pub struct RestoreResult {
    pub _success: bool,
    pub message: String,
    pub _manifest: Option<Manifest>,
    pub _identity_restored: bool,
    pub _config_restored: bool,
    pub needs_restart: bool,
}

/// Restore backup handler - processes uploaded tar.gz backup
pub async fn restore_backup(
    State(state): State<AppState>,
    Extension(user): Extension<User>,
    mut multipart: Multipart,
) -> Response {
    // Only admins can restore backups
    if !permissions::is_admin(&user) {
        return (StatusCode::FORBIDDEN, "Only administrators can restore backups").into_response();
    }

    // Read the uploaded file
    let mut file_data: Option<Vec<u8>> = None;

    while let Ok(Some(field)) = multipart.next_field().await {
        if field.name() == Some("backup_file") {
            match field.bytes().await {
                Ok(bytes) => {
                    file_data = Some(bytes.to_vec());
                    break;
                }
                Err(e) => {
                    return restore_error_response(&format!("Failed to read upload: {}", e));
                }
            }
        }
    }

    let file_data = match file_data {
        Some(d) => d,
        None => {
            return restore_error_response("No backup file provided");
        }
    };

    // Extract archive
    let temp_dir = match extract_tar_gz(&file_data) {
        Ok(d) => d,
        Err(e) => {
            return restore_error_response(&format!("Failed to extract archive: {}", e));
        }
    };

    // Find the backup directory (it's usually named tierdrop-backup-TIMESTAMP)
    let backup_dir = find_backup_dir(temp_dir.path());
    let backup_path = match backup_dir {
        Some(p) => p,
        None => {
            return restore_error_response("Invalid backup archive: no backup directory found");
        }
    };

    // Read and validate manifest
    let manifest_path = backup_path.join("manifest.json");
    let manifest: Manifest = if manifest_path.exists() {
        match std::fs::read_to_string(&manifest_path) {
            Ok(content) => match serde_json::from_str(&content) {
                Ok(m) => m,
                Err(e) => {
                    return restore_error_response(&format!("Invalid manifest: {}", e));
                }
            },
            Err(e) => {
                return restore_error_response(&format!("Failed to read manifest: {}", e));
            }
        }
    } else {
        return restore_error_response("Invalid backup: manifest.json not found");
    };

    // Restore ZeroTier directory
    let zt_source = backup_path.join("zerotier-one");
    let zt_dest = zerotier_data_dir();
    let mut identity_restored = false;
    let mut authtoken_restored = false;

    if zt_source.exists() {
        // Clear existing controller.d and copy new one
        let controller_d_dest = zt_dest.join("controller.d");
        let controller_d_source = zt_source.join("controller.d");

        if controller_d_source.exists() {
            // Remove existing controller.d
            let _ = std::fs::remove_dir_all(&controller_d_dest);
            if let Err(e) = copy_dir_recursive(&controller_d_source, &controller_d_dest) {
                tracing::error!("Failed to restore controller.d: {}", e);
            }
        }

        // Try to restore identity files
        let secret_source = zt_source.join("identity.secret");
        let public_source = zt_source.join("identity.public");

        if secret_source.exists() && public_source.exists() {
            let secret_ok = std::fs::copy(&secret_source, zt_dest.join("identity.secret")).is_ok();
            let public_ok = std::fs::copy(&public_source, zt_dest.join("identity.public")).is_ok();
            identity_restored = secret_ok && public_ok;
        }

        // Restore authtoken.secret
        let authtoken_source = zt_source.join("authtoken.secret");
        if authtoken_source.exists() {
            authtoken_restored = std::fs::copy(&authtoken_source, zt_dest.join("authtoken.secret")).is_ok();
        }
    }

    // Read the new auth token from the restored ZeroTier directory
    let new_auth_token = std::fs::read_to_string(zt_dest.join("authtoken.secret"))
        .map(|s| s.trim().to_string())
        .ok();

    // Restore TierDrop config
    let config_path = backup_path.join("tierdrop-config.json");
    let config_restored = if config_path.exists() {
        match std::fs::read_to_string(&config_path) {
            Ok(content) => {
                match serde_json::from_str::<crate::state::Config>(&content) {
                    Ok(mut restored_config) => {
                        // Update the auth token to match the restored ZeroTier directory
                        if let Some(ref token) = new_auth_token {
                            restored_config.zt_token = token.clone();
                        }

                        // Update state and save
                        {
                            let mut cfg = state.config.write().await;
                            *cfg = Some(restored_config.clone());
                        }
                        match restored_config.save() {
                            Ok(_) => true,
                            Err(e) => {
                                tracing::error!("Failed to save restored config: {}", e);
                                false
                            }
                        }
                    }
                    Err(e) => {
                        tracing::error!("Failed to parse restored config: {}", e);
                        false
                    }
                }
            }
            Err(e) => {
                tracing::error!("Failed to read restored config: {}", e);
                false
            }
        }
    } else {
        // No TierDrop config in backup, but we still need to update the auth token
        // in the current config if we restored the authtoken.secret
        if let Some(ref token) = new_auth_token {
            let mut cfg = state.config.write().await;
            if let Some(ref mut c) = *cfg {
                c.zt_token = token.clone();
                let _ = c.save();
            }
        }
        false
    };

    // Reinitialize ZtClient with the new auth token
    if let Some(ref token) = new_auth_token {
        let base_url = {
            let cfg = state.config.read().await;
            cfg.as_ref()
                .map(|c| c.zt_base_url.clone())
                .unwrap_or_else(|| "http://localhost:9993".to_string())
        };
        let base_url = std::env::var("ZT_BASE_URL").unwrap_or(base_url);
        let new_client = ZtClient::new(base_url, token.clone());
        {
            let mut client = state.zt_client.write().await;
            *client = Some(new_client);
        }
        // Trigger an immediate poll to refresh data with the new client
        state.notify_poller();
    }

    // Build result message
    let mut messages: Vec<String> = Vec::new();
    if identity_restored {
        messages.push("Identity files restored".to_string());
    }
    if authtoken_restored {
        messages.push("Auth token restored".to_string());
    }
    if config_restored {
        messages.push("TierDrop config restored".to_string());
    }

    // Needs restart if identity was restored (ZeroTier service needs to pick up new identity)
    let needs_restart = identity_restored;

    let result = RestoreResult {
        _success: true,
        message: messages.join(". "),
        _manifest: Some(manifest),
        _identity_restored: identity_restored,
        _config_restored: config_restored,
        needs_restart,
    };

    restore_success_response(result)
}

fn find_backup_dir(temp_dir: &Path) -> Option<PathBuf> {
    // Look for a directory starting with "tierdrop-backup-"
    if let Ok(entries) = std::fs::read_dir(temp_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                let name = entry.file_name().to_string_lossy().to_string();
                if name.starts_with("tierdrop-backup-") {
                    return Some(path);
                }
            }
        }
    }
    // Maybe files are directly in temp_dir
    if temp_dir.join("manifest.json").exists() {
        return Some(temp_dir.to_path_buf());
    }
    None
}

fn restore_error_response(message: &str) -> Response {
    let html = format!(
        r#"<div class="restore-result error">
            <div class="restore-icon">!</div>
            <h4>Restore Failed</h4>
            <p>{}</p>
        </div>"#,
        message
    );
    Response::builder()
        .header(CONTENT_TYPE, "text/html")
        .body(Body::from(html))
        .unwrap()
}

fn restore_success_response(result: RestoreResult) -> Response {
    let restart_notice = if result.needs_restart {
        r#"<p class="restore-notice"><strong>Important:</strong> ZeroTier service and TierDrop both need to be restarted for identity changes to take effect.</p>"#
    } else {
        r#"<p class="restore-notice"><strong>Note:</strong> Restart TierDrop to fully apply the restored configuration.</p>"#
    };

    let html = format!(
        r#"<div class="restore-result success">
            <div class="restore-icon">✓</div>
            <h4>Restore Successful</h4>
            <p>{}</p>
            {}
        </div>"#,
        result.message, restart_notice
    );
    Response::builder()
        .header(CONTENT_TYPE, "text/html")
        .body(Body::from(html))
        .unwrap()
}

/// Get backup status info for the settings page
pub struct BackupStatus {
    pub node_address: Option<String>,
    pub network_count: usize,
    pub can_backup_identity: bool,
}

impl BackupStatus {
    pub async fn fetch(state: &AppState) -> Self {
        let zt = state.zt_state.read().await;
        let node_address = zt.status.as_ref().and_then(|s| s.address.clone());
        let network_count = zt.controller_networks.len();
        drop(zt);

        // Check if we can read identity files
        let zt_dir = zerotier_data_dir();
        let can_backup_identity = zt_dir.join("identity.secret").exists()
            && std::fs::metadata(zt_dir.join("identity.secret"))
                .map(|m| m.permissions().readonly() == false)
                .unwrap_or(false);

        Self {
            node_address,
            network_count,
            can_backup_identity,
        }
    }

    pub fn backup_type(&self) -> &'static str {
        if self.can_backup_identity {
            "full"
        } else {
            "partial"
        }
    }
}
