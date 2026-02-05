use askama::Template;
use askama_web::WebTemplate;
use axum::extract::State;
use axum::response::IntoResponse;

use crate::state::AppState;
use crate::zt::models::{ControllerNetwork, NodeStatus};

/// Network row data passed to the dashboard template
pub struct NetworkRow {
    pub network: ControllerNetwork,
    pub member_count: usize,
}

#[derive(Template, WebTemplate)]
#[template(path = "dashboard.html")]
pub struct DashboardTemplate {
    pub status: Option<NodeStatus>,
    pub network_rows: Vec<NetworkRow>,
    pub network_count: usize,
    pub total_members: usize,
    pub authorized_members: usize,
    pub error: Option<String>,
}

pub async fn dashboard(State(state): State<AppState>) -> impl IntoResponse {
    let zt = state.zt_state.read().await;
    let total_members: usize = zt.controller_members.values().map(|v| v.len()).sum();
    let authorized_members: usize = zt
        .controller_members
        .values()
        .flat_map(|v| v.iter())
        .filter(|m| m.is_authorized())
        .count();
    let network_rows: Vec<NetworkRow> = zt
        .controller_networks
        .iter()
        .map(|net| {
            let nwid = net.display_id().to_string();
            let member_count = zt
                .controller_members
                .get(&nwid)
                .map(|v| v.len())
                .unwrap_or(0);
            NetworkRow {
                network: net.clone(),
                member_count,
            }
        })
        .collect();
    DashboardTemplate {
        status: zt.status.clone(),
        network_count: zt.controller_networks.len(),
        network_rows,
        total_members,
        authorized_members,
        error: zt.error.clone(),
    }
}

#[derive(Template, WebTemplate)]
#[template(path = "partials/dashboard_stats.html")]
pub struct DashboardStatsPartial {
    pub status: Option<NodeStatus>,
    pub network_count: usize,
    pub total_members: usize,
    pub authorized_members: usize,
    pub error: Option<String>,
}

pub async fn dashboard_partial(State(state): State<AppState>) -> impl IntoResponse {
    let zt = state.zt_state.read().await;
    let total_members: usize = zt.controller_members.values().map(|v| v.len()).sum();
    let authorized_members: usize = zt
        .controller_members
        .values()
        .flat_map(|v| v.iter())
        .filter(|m| m.is_authorized())
        .count();
    DashboardStatsPartial {
        status: zt.status.clone(),
        network_count: zt.controller_networks.len(),
        total_members,
        authorized_members,
        error: zt.error.clone(),
    }
}

/// Network list partial for SSE refresh
#[derive(Template, WebTemplate)]
#[template(path = "partials/dashboard_networks.html")]
pub struct DashboardNetworksPartial {
    pub network_rows: Vec<NetworkRow>,
}

pub async fn dashboard_networks_partial(State(state): State<AppState>) -> impl IntoResponse {
    let zt = state.zt_state.read().await;
    let network_rows: Vec<NetworkRow> = zt
        .controller_networks
        .iter()
        .map(|net| {
            let nwid = net.display_id().to_string();
            let member_count = zt
                .controller_members
                .get(&nwid)
                .map(|v| v.len())
                .unwrap_or(0);
            NetworkRow {
                network: net.clone(),
                member_count,
            }
        })
        .collect();
    DashboardNetworksPartial { network_rows }
}
