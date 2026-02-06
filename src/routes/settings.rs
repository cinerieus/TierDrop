use askama::Template;
use askama_web::WebTemplate;
use axum::extract::State;
use axum::response::{Html, IntoResponse};
use axum::Form;
use serde::Deserialize;

use crate::auth::{hash_password, verify_password};
use crate::routes::backup::BackupStatus;
use crate::state::AppState;

#[derive(Template, WebTemplate)]
#[template(path = "settings.html")]
pub struct SettingsTemplate {
    pub node_address: Option<String>,
    pub network_count: usize,
    pub backup_type: String,
    pub version: &'static str,
}

pub async fn settings_page(State(state): State<AppState>) -> impl IntoResponse {
    let status = BackupStatus::fetch(&state).await;
    let backup_type = status.backup_type().to_string();

    SettingsTemplate {
        node_address: status.node_address,
        network_count: status.network_count,
        backup_type,
        version: crate::VERSION,
    }
}

#[derive(Deserialize)]
pub struct PasswordChangeForm {
    current_password: String,
    new_password: String,
    confirm_password: String,
}

pub async fn change_password(
    State(state): State<AppState>,
    Form(form): Form<PasswordChangeForm>,
) -> impl IntoResponse {
    // Validate new password matches confirmation
    if form.new_password != form.confirm_password {
        return Html(r#"<div class="password-result error">Passwords do not match.</div>"#.to_string());
    }

    // Validate new password length
    if form.new_password.len() < 4 {
        return Html(r#"<div class="password-result error">Password must be at least 4 characters.</div>"#.to_string());
    }

    // Get current password hash from config
    let config = state.config.read().await;
    let current_hash = match config.as_ref() {
        Some(c) => c.password_hash.clone(),
        None => {
            return Html(r#"<div class="password-result error">No configuration found.</div>"#.to_string());
        }
    };
    drop(config);

    // Verify current password
    if !verify_password(&form.current_password, &current_hash) {
        return Html(r#"<div class="password-result error">Current password is incorrect.</div>"#.to_string());
    }

    // Hash new password
    let new_hash = match hash_password(&form.new_password) {
        Ok(h) => h,
        Err(e) => {
            return Html(format!(r#"<div class="password-result error">Failed to hash password: {}</div>"#, e));
        }
    };

    // Update config with new password
    let mut config = state.config.write().await;
    if let Some(ref mut c) = *config {
        c.password_hash = new_hash;
        if let Err(e) = c.save() {
            return Html(format!(r#"<div class="password-result error">Failed to save config: {}</div>"#, e));
        }
    }

    Html(r#"<div class="password-result success">Password changed successfully.</div>"#.to_string())
}
