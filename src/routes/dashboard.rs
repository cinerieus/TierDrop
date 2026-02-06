use askama::Template;
use askama_web::WebTemplate;
use axum::extract::State;
use axum::response::IntoResponse;
use axum::Extension;

use crate::permissions;
use crate::state::{AppState, User};
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
    pub version: &'static str,
}

pub async fn dashboard(
    State(state): State<AppState>,
    Extension(user): Extension<User>,
) -> impl IntoResponse {
    let zt = state.zt_state.read().await;

    // Filter networks based on user permissions
    let visible_networks: Vec<&ControllerNetwork> = zt
        .controller_networks
        .iter()
        .filter(|net| permissions::can_read(&user, net.display_id()))
        .collect();

    // Calculate member stats only for visible networks
    let total_members: usize = visible_networks
        .iter()
        .map(|net| {
            zt.controller_members
                .get(net.display_id())
                .map(|v| v.len())
                .unwrap_or(0)
        })
        .sum();
    let authorized_members: usize = visible_networks
        .iter()
        .flat_map(|net| {
            zt.controller_members
                .get(net.display_id())
                .map(|v| v.iter())
                .into_iter()
                .flatten()
        })
        .filter(|m| m.is_authorized())
        .count();

    let network_rows: Vec<NetworkRow> = visible_networks
        .iter()
        .map(|net| {
            let nwid = net.display_id().to_string();
            let member_count = zt
                .controller_members
                .get(&nwid)
                .map(|v| v.len())
                .unwrap_or(0);
            NetworkRow {
                network: (*net).clone(),
                member_count,
            }
        })
        .collect();
    DashboardTemplate {
        status: zt.status.clone(),
        network_count: visible_networks.len(),
        network_rows,
        total_members,
        authorized_members,
        error: zt.error.clone(),
        version: crate::VERSION,
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

pub async fn dashboard_partial(
    State(state): State<AppState>,
    Extension(user): Extension<User>,
) -> impl IntoResponse {
    let zt = state.zt_state.read().await;

    // Filter networks based on user permissions
    let visible_networks: Vec<&ControllerNetwork> = zt
        .controller_networks
        .iter()
        .filter(|net| permissions::can_read(&user, net.display_id()))
        .collect();

    let total_members: usize = visible_networks
        .iter()
        .map(|net| {
            zt.controller_members
                .get(net.display_id())
                .map(|v| v.len())
                .unwrap_or(0)
        })
        .sum();
    let authorized_members: usize = visible_networks
        .iter()
        .flat_map(|net| {
            zt.controller_members
                .get(net.display_id())
                .map(|v| v.iter())
                .into_iter()
                .flatten()
        })
        .filter(|m| m.is_authorized())
        .count();

    DashboardStatsPartial {
        status: zt.status.clone(),
        network_count: visible_networks.len(),
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

pub async fn dashboard_networks_partial(
    State(state): State<AppState>,
    Extension(user): Extension<User>,
) -> impl IntoResponse {
    let zt = state.zt_state.read().await;
    let network_rows: Vec<NetworkRow> = zt
        .controller_networks
        .iter()
        .filter(|net| permissions::can_read(&user, net.display_id()))
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
