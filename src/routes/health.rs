use axum::extract::State;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::Serialize;

use crate::state::AppState;

#[derive(Serialize)]
pub struct HealthResponse {
    pub status: &'static str,
    pub zt_connected: bool,
    pub version: &'static str,
}

pub async fn health_check(State(state): State<AppState>) -> Response {
    // Check if ZtClient can reach ZeroTier API by checking if we have status
    let zt = state.zt_state.read().await;
    let zt_connected = zt.status.is_some() && zt.error.is_none();

    let response = HealthResponse {
        status: if zt_connected { "healthy" } else { "degraded" },
        zt_connected,
        version: crate::VERSION,
    };

    // Return 200 if healthy, 503 if ZT unreachable
    if zt_connected {
        Json(response).into_response()
    } else {
        (StatusCode::SERVICE_UNAVAILABLE, Json(response)).into_response()
    }
}
