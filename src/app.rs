use axum::middleware;
use axum::routing::{delete, get, post};
use axum::Router;
use tower_sessions::cookie::time::Duration;
use tower_sessions::{Expiry, MemoryStore, SessionManagerLayer};

use crate::assets::serve_static;
use crate::auth;
use crate::routes::{backup, controller, dashboard, health, settings};
use crate::sse;
use crate::state::AppState;

pub fn build_router(state: AppState) -> Router {
    let session_store = MemoryStore::default();
    let session_layer = SessionManagerLayer::new(session_store)
        .with_secure(false) // Allow HTTP for local use
        .with_expiry(Expiry::OnInactivity(Duration::minutes(30)));

    // Routes that require authentication
    let protected = Router::new()
        .route("/", get(dashboard::dashboard))
        .route("/events", get(sse::sse_handler))
        // Dashboard partials
        .route("/partials/dashboard", get(dashboard::dashboard_partial))
        .route("/partials/networks", get(dashboard::dashboard_networks_partial))
        // Controller pages
        .route("/controller/create", post(controller::create_network))
        .route(
            "/controller/{nwid}",
            get(controller::controller_network_detail),
        )
        .route(
            "/controller/{nwid}",
            delete(controller::delete_network),
        )
        // Controller network config
        .route(
            "/controller/{nwid}/settings",
            post(controller::update_settings),
        )
        .route(
            "/controller/{nwid}/assign-modes",
            post(controller::update_assign_modes),
        )
        .route(
            "/controller/{nwid}/broadcast-settings",
            post(controller::update_broadcast_settings),
        )
        .route("/controller/{nwid}/pools", post(controller::add_pool))
        .route(
            "/controller/{nwid}/pools/remove",
            post(controller::remove_pool),
        )
        .route("/controller/{nwid}/routes", post(controller::add_route))
        .route(
            "/controller/{nwid}/routes/remove",
            post(controller::remove_route),
        )
        .route("/controller/{nwid}/dns", post(controller::add_dns))
        .route(
            "/controller/{nwid}/dns/remove",
            post(controller::remove_dns),
        )
        .route(
            "/controller/{nwid}/flow-rules",
            post(controller::update_flow_rules),
        )
        // Controller member actions
        .route(
            "/controller/{nwid}/members/add",
            post(controller::add_member),
        )
        .route(
            "/controller/{nwid}/members/{member_id}/authorize",
            post(controller::toggle_member_auth),
        )
        .route(
            "/controller/{nwid}/members/{member_id}/modal",
            get(controller::member_modal),
        )
        .route(
            "/controller/{nwid}/members/{member_id}/update",
            post(controller::update_member),
        )
        .route(
            "/controller/{nwid}/members/{member_id}",
            delete(controller::delete_member),
        )
        // Controller SSE partials
        .route(
            "/controller/partials/{nwid}/members",
            get(controller::ctrl_member_list_partial),
        )
        // Settings and backup
        .route("/settings", get(settings::settings_page))
        .route("/settings/password", post(settings::change_password))
        .route("/settings/username", post(settings::change_username))
        .route("/settings/backup/export", post(backup::export_backup))
        .route("/settings/backup/restore", post(backup::restore_backup))
        // User management (admin only)
        .route("/settings/users", get(settings::users_list))
        .route("/settings/users/create", post(settings::create_user))
        .route("/settings/users/{id}/modal", get(settings::user_modal))
        .route("/settings/users/{id}/update", post(settings::update_user))
        .route("/settings/users/{id}", delete(settings::delete_user))
        // 2FA settings
        .route("/settings/2fa/setup", get(settings::totp_setup_modal))
        .route("/settings/2fa/enable", post(settings::totp_enable))
        .route("/settings/2fa/disable-modal", get(settings::totp_disable_modal))
        .route("/settings/2fa/disable", post(settings::totp_disable))
        .route("/settings/2fa/status", get(settings::totp_status))
        .layer(middleware::from_fn_with_state(
            state.clone(),
            auth::auth_middleware,
        ));

    // Public routes
    let public = Router::new()
        .route("/health", get(health::health_check))
        .route("/setup", get(auth::setup_page))
        .route("/setup", post(auth::setup_submit))
        .route("/login", get(auth::login_page))
        .route("/login", post(auth::login_submit))
        .route("/login/2fa", get(auth::login_2fa_page))
        .route("/login/2fa", post(auth::login_2fa_submit))
        .route("/logout", get(auth::logout))
        .route("/static/{*path}", get(serve_static));

    Router::new()
        .merge(protected)
        .merge(public)
        .layer(session_layer)
        .with_state(state)
}
