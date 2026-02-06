use axum::middleware;
use axum::routing::{delete, get, post};
use axum::Router;
use tower_sessions::cookie::time::Duration;
use tower_sessions::{Expiry, MemoryStore, SessionManagerLayer};

use crate::assets::serve_static;
use crate::auth;
use crate::routes::{controller, dashboard};
use crate::sse;
use crate::state::AppState;

pub fn build_router(state: AppState) -> Router {
    let session_store = MemoryStore::default();
    let session_layer = SessionManagerLayer::new(session_store)
        .with_secure(false) // Allow HTTP for local use
        .with_expiry(Expiry::OnInactivity(Duration::hours(24)));

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
        .layer(middleware::from_fn_with_state(
            state.clone(),
            auth::auth_middleware,
        ));

    // Public routes
    let public = Router::new()
        .route("/setup", get(auth::setup_page))
        .route("/setup", post(auth::setup_submit))
        .route("/login", get(auth::login_page))
        .route("/login", post(auth::login_submit))
        .route("/logout", get(auth::logout))
        .route("/static/{*path}", get(serve_static));

    Router::new()
        .merge(protected)
        .merge(public)
        .layer(session_layer)
        .with_state(state)
}
