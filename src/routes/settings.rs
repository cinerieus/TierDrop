use std::collections::HashMap;

use askama::Template;
use askama_web::WebTemplate;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::{Html, IntoResponse, Response};
use axum::Form;
use axum::Extension;
use serde::Deserialize;

use crate::auth::{hash_password, verify_password};
use crate::routes::backup::BackupStatus;
use crate::state::{AppState, NetworkPermissions, User};
use crate::zt::models::ControllerNetwork;

#[derive(Template, WebTemplate)]
#[template(path = "settings.html")]
pub struct SettingsTemplate {
    pub node_address: Option<String>,
    pub network_count: usize,
    pub backup_type: String,
    pub version: &'static str,
    pub is_admin: bool,
    pub users: Vec<User>,
    pub current_username: String,
}

pub async fn settings_page(
    State(state): State<AppState>,
    Extension(current_user): Extension<User>,
) -> impl IntoResponse {
    let status = BackupStatus::fetch(&state).await;
    let backup_type = status.backup_type().to_string();

    let users = {
        let config = state.config.read().await;
        config.as_ref().map(|c| c.users.clone()).unwrap_or_default()
    };

    SettingsTemplate {
        node_address: status.node_address,
        network_count: status.network_count,
        backup_type,
        version: crate::VERSION,
        is_admin: current_user.is_admin,
        users,
        current_username: current_user.username,
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
    Extension(current_user): Extension<User>,
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

    // Verify current password
    if !verify_password(&form.current_password, &current_user.password_hash) {
        return Html(r#"<div class="password-result error">Current password is incorrect.</div>"#.to_string());
    }

    // Hash new password
    let new_hash = match hash_password(&form.new_password) {
        Ok(h) => h,
        Err(e) => {
            return Html(format!(r#"<div class="password-result error">Failed to hash password: {}</div>"#, e));
        }
    };

    // Update current user's password in config
    let mut config = state.config.write().await;
    if let Some(ref mut c) = *config {
        if let Some(user) = c.find_user_by_id_mut(current_user.id) {
            user.password_hash = new_hash;
            if let Err(e) = c.save() {
                return Html(format!(r#"<div class="password-result error">Failed to save config: {}</div>"#, e));
            }
        } else {
            return Html(r#"<div class="password-result error">User not found.</div>"#.to_string());
        }
    }

    Html(r#"<div class="password-result success">Password changed successfully.</div>"#.to_string())
}

#[derive(Deserialize)]
pub struct UsernameChangeForm {
    new_username: String,
}

pub async fn change_username(
    State(state): State<AppState>,
    Extension(current_user): Extension<User>,
    Form(form): Form<UsernameChangeForm>,
) -> impl IntoResponse {
    let new_username = form.new_username.trim().to_string();

    if new_username.is_empty() {
        return Html(r#"<div class="username-result error">Username is required.</div>"#.to_string());
    }

    // Check if username already exists (by another user)
    {
        let config = state.config.read().await;
        if let Some(c) = config.as_ref() {
            if let Some(existing) = c.find_user_by_username(&new_username) {
                if existing.id != current_user.id {
                    return Html(r#"<div class="username-result error">Username already taken.</div>"#.to_string());
                }
            }
        }
    }

    // Update username
    let mut config = state.config.write().await;
    if let Some(ref mut c) = *config {
        if let Some(user) = c.find_user_by_id_mut(current_user.id) {
            user.username = new_username;
            if let Err(e) = c.save() {
                return Html(format!(r#"<div class="username-result error">Failed to save config: {}</div>"#, e));
            }
        } else {
            return Html(r#"<div class="username-result error">User not found.</div>"#.to_string());
        }
    }

    Html(r#"<div class="username-result success">Username changed successfully.</div>"#.to_string())
}

// ---- Users Management (Admin only) ----

#[derive(Template, WebTemplate)]
#[template(path = "partials/users_list.html")]
pub struct UsersListTemplate {
    pub users: Vec<User>,
    pub current_user_id: u64,
}

/// GET /settings/users - Users list partial
pub async fn users_list(
    State(state): State<AppState>,
    Extension(current_user): Extension<User>,
) -> Response {
    if !current_user.is_admin {
        return (StatusCode::FORBIDDEN, "Admin access required").into_response();
    }

    let users = {
        let config = state.config.read().await;
        config.as_ref().map(|c| c.users.clone()).unwrap_or_default()
    };

    UsersListTemplate {
        users,
        current_user_id: current_user.id,
    }.into_response()
}

#[derive(Deserialize)]
pub struct CreateUserForm {
    username: String,
    password: String,
    #[serde(default)]
    is_admin: Option<String>,
}

/// POST /settings/users/create - Create new user
pub async fn create_user(
    State(state): State<AppState>,
    Extension(current_user): Extension<User>,
    Form(form): Form<CreateUserForm>,
) -> Response {
    if !current_user.is_admin {
        return (StatusCode::FORBIDDEN, "Admin access required").into_response();
    }

    let username = form.username.trim().to_string();
    if username.is_empty() {
        return Html(r#"<div class="alert alert-error mb-4">Username is required.</div>"#.to_string()).into_response();
    }

    if form.password.len() < 4 {
        return Html(r#"<div class="alert alert-error mb-4">Password must be at least 4 characters.</div>"#.to_string()).into_response();
    }

    // Check if username already exists
    {
        let config = state.config.read().await;
        if let Some(c) = config.as_ref() {
            if c.find_user_by_username(&username).is_some() {
                return Html(r#"<div class="alert alert-error mb-4">Username already exists.</div>"#.to_string()).into_response();
            }
        }
    }

    let password_hash = match hash_password(&form.password) {
        Ok(h) => h,
        Err(e) => {
            return Html(format!(r#"<div class="alert alert-error mb-4">Failed to hash password: {}</div>"#, e)).into_response();
        }
    };

    let is_admin = form.is_admin.as_deref() == Some("true");

    let users = {
        let mut config = state.config.write().await;
        if let Some(ref mut c) = *config {
            c.add_user(username, password_hash, is_admin);
            if let Err(e) = c.save() {
                return Html(format!(r#"<div class="alert alert-error mb-4">Failed to save: {}</div>"#, e)).into_response();
            }
            c.users.clone()
        } else {
            return Html(r#"<div class="alert alert-error mb-4">No configuration found.</div>"#.to_string()).into_response();
        }
    };

    UsersListTemplate {
        users,
        current_user_id: current_user.id,
    }.into_response()
}

#[derive(Template, WebTemplate)]
#[template(path = "partials/user_modal.html")]
pub struct UserModalTemplate {
    pub user: User,
    pub networks: Vec<ControllerNetwork>,
}

/// GET /settings/users/{id}/modal - User edit modal
pub async fn user_modal(
    State(state): State<AppState>,
    Extension(current_user): Extension<User>,
    Path(user_id): Path<u64>,
) -> Response {
    if !current_user.is_admin {
        return (StatusCode::FORBIDDEN, "Admin access required").into_response();
    }

    let (user, networks) = {
        let config = state.config.read().await;
        let user = config.as_ref()
            .and_then(|c| c.find_user_by_id(user_id).cloned());

        let zt = state.zt_state.read().await;
        let networks = zt.controller_networks.clone();

        (user, networks)
    };

    match user {
        Some(user) => UserModalTemplate { user, networks }.into_response(),
        None => (StatusCode::NOT_FOUND, "User not found").into_response(),
    }
}

#[derive(Deserialize)]
pub struct UpdateUserForm {
    username: String,
    #[serde(default)]
    password: String,
    #[serde(default)]
    is_admin: Option<String>,
    #[serde(flatten)]
    permissions: HashMap<String, String>,
}

/// POST /settings/users/{id}/update - Update user
pub async fn update_user(
    State(state): State<AppState>,
    Extension(current_user): Extension<User>,
    Path(user_id): Path<u64>,
    Form(form): Form<UpdateUserForm>,
) -> Response {
    if !current_user.is_admin {
        return (StatusCode::FORBIDDEN, "Admin access required").into_response();
    }

    let username = form.username.trim().to_string();
    if username.is_empty() {
        return Html(r#"<div class="alert alert-error">Username is required.</div>"#.to_string()).into_response();
    }

    // Get networks for building permissions (use display_id which handles Option<String>)
    let networks: Vec<String> = {
        let zt = state.zt_state.read().await;
        zt.controller_networks.iter().map(|n| n.display_id().to_string()).collect()
    };

    let users = {
        let mut config = state.config.write().await;
        if let Some(ref mut c) = *config {
            // Check if username is taken by another user
            if let Some(existing) = c.find_user_by_username(&username) {
                if existing.id != user_id {
                    return Html(r#"<div class="alert alert-error">Username already taken.</div>"#.to_string()).into_response();
                }
            }

            if let Some(user) = c.find_user_by_id_mut(user_id) {
                user.username = username;

                // Update password if provided
                if !form.password.is_empty() {
                    if form.password.len() < 4 {
                        return Html(r#"<div class="alert alert-error">Password must be at least 4 characters.</div>"#.to_string()).into_response();
                    }
                    match hash_password(&form.password) {
                        Ok(h) => user.password_hash = h,
                        Err(e) => {
                            return Html(format!(r#"<div class="alert alert-error">Failed to hash password: {}</div>"#, e)).into_response();
                        }
                    }
                }

                user.is_admin = form.is_admin.as_deref() == Some("true");

                // Build network permissions from form
                // Form fields are like: perm_NWID_read, perm_NWID_authorize, etc.
                user.network_permissions.clear();
                for nwid in &networks {
                    let read = form.permissions.contains_key(&format!("perm_{}_read", nwid));
                    let authorize = form.permissions.contains_key(&format!("perm_{}_authorize", nwid));
                    let modify = form.permissions.contains_key(&format!("perm_{}_modify", nwid));
                    let delete = form.permissions.contains_key(&format!("perm_{}_delete", nwid));

                    if read || authorize || modify || delete {
                        user.network_permissions.insert(nwid.clone(), NetworkPermissions {
                            read,
                            authorize,
                            modify,
                            delete,
                        });
                    }
                }

                if let Err(e) = c.save() {
                    return Html(format!(r#"<div class="alert alert-error">Failed to save: {}</div>"#, e)).into_response();
                }
            } else {
                return (StatusCode::NOT_FOUND, "User not found").into_response();
            }
            c.users.clone()
        } else {
            return Html(r#"<div class="alert alert-error">No configuration found.</div>"#.to_string()).into_response();
        }
    };

    // Return updated users list with HX-Trigger to close modal
    let html = UsersListTemplate {
        users,
        current_user_id: current_user.id,
    };

    (
        [("HX-Trigger", "closeModal")],
        html,
    ).into_response()
}

/// DELETE /settings/users/{id} - Delete user
pub async fn delete_user(
    State(state): State<AppState>,
    Extension(current_user): Extension<User>,
    Path(user_id): Path<u64>,
) -> Response {
    if !current_user.is_admin {
        return (StatusCode::FORBIDDEN, "Admin access required").into_response();
    }

    // Prevent self-deletion
    if user_id == current_user.id {
        return Html(r#"<div class="alert alert-error">Cannot delete your own account.</div>"#.to_string()).into_response();
    }

    let users = {
        let mut config = state.config.write().await;
        if let Some(ref mut c) = *config {
            // Check if this is the last admin
            let target_is_admin = c.find_user_by_id(user_id).map(|u| u.is_admin).unwrap_or(false);
            let admin_count = c.users.iter().filter(|u| u.is_admin).count();

            if target_is_admin && admin_count <= 1 {
                return Html(r#"<div class="alert alert-error">Cannot delete the last admin user.</div>"#.to_string()).into_response();
            }

            if !c.remove_user(user_id) {
                return (StatusCode::NOT_FOUND, "User not found").into_response();
            }

            if let Err(e) = c.save() {
                return Html(format!(r#"<div class="alert alert-error">Failed to save: {}</div>"#, e)).into_response();
            }
            c.users.clone()
        } else {
            return Html(r#"<div class="alert alert-error">No configuration found.</div>"#.to_string()).into_response();
        }
    };

    UsersListTemplate {
        users,
        current_user_id: current_user.id,
    }.into_response()
}
