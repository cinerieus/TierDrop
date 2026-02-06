use askama::Template;
use askama_web::WebTemplate;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Redirect, Response};
use axum::Form;
use axum::Extension;
use serde::Deserialize;

use crate::permissions;
use crate::state::{AppState, User};
use crate::zt::models::{ControllerMember, ControllerNetwork, ControllerRoute, IpAssignmentPool};

// ---- Default Flow Rules ----

/// Default rules DSL source (same as shown in editor)
pub const DEFAULT_RULES_SOURCE: &str = r#"#
# This is a default rule set that allows IPv4 and IPv6 traffic but otherwise
# behaves like a standard Ethernet switch:

drop
	not ethertype ipv4
	and not ethertype arp
	and not ethertype ipv6
;

# Accept anything else. This is required since default is 'drop':

accept;

# For more information on how rules work visit: https://docs.zerotier.com/rules/
"#;

/// Returns the compiled default rules as JSON array
fn default_compiled_rules() -> Vec<serde_json::Value> {
    // Pre-compiled default rules (drop non-IP traffic, accept everything else)
    serde_json::from_str(
        r#"[
        {"not":true,"or":false,"type":"MATCH_ETHERTYPE","etherType":2048},
        {"not":true,"or":false,"type":"MATCH_ETHERTYPE","etherType":2054},
        {"not":true,"or":false,"type":"MATCH_ETHERTYPE","etherType":34525},
        {"type":"ACTION_DROP"},
        {"type":"ACTION_ACCEPT"}
    ]"#,
    )
    .unwrap_or_default()
}

// ---- Display row with enriched data ----

pub struct MemberDisplayRow {
    pub member: ControllerMember,
    pub name: String,
    pub rfc4193_addr: Option<String>,
    pub sixplane_addr: Option<String>,
}

/// Build enriched member rows from raw members + local names.
fn enrich_members(
    members: &[ControllerMember],
    member_names: &std::collections::HashMap<String, String>,
    network: &ControllerNetwork,
) -> Vec<MemberDisplayRow> {
    let show_rfc4193 = network.v6_rfc4193();
    let show_sixplane = network.v6_sixplane();

    members
        .iter()
        .map(|m| {
            let name = member_names
                .get(m.display_id())
                .cloned()
                .unwrap_or_default();
            MemberDisplayRow {
                rfc4193_addr: if show_rfc4193 { m.rfc4193_address() } else { None },
                sixplane_addr: if show_sixplane { m.sixplane_address() } else { None },
                member: m.clone(),
                name,
            }
        })
        .collect()
}

// ---- Page Templates ----

#[derive(Template, WebTemplate)]
#[template(path = "controller/network_detail.html")]
pub struct ControllerNetworkDetailTemplate {
    pub network: ControllerNetwork,
    pub rows: Vec<MemberDisplayRow>,
    pub member_count: usize,
    pub authorized_count: usize,
    pub nwid: String,
    pub pools: Vec<IpAssignmentPool>,
    pub routes: Vec<ControllerRoute>,
    pub rules_source: String,
    pub is_htmx: bool,
    pub version: &'static str,
    // Permissions
    pub can_authorize: bool,
    pub can_modify: bool,
    pub can_delete: bool,
}

// ---- Partial Templates ----

#[derive(Template, WebTemplate)]
#[template(path = "controller/partials/member_list.html")]
pub struct CtrlMemberListPartial {
    pub nwid: String,
    pub rows: Vec<MemberDisplayRow>,
    pub member_count: usize,
    pub authorized_count: usize,
    pub is_htmx: bool,
    pub can_authorize: bool,
    pub can_modify: bool,
}

#[derive(Template, WebTemplate)]
#[template(path = "controller/partials/network_settings.html")]
pub struct CtrlNetworkSettingsPartial {
    pub network: ControllerNetwork,
    pub can_modify: bool,
}

#[derive(Template, WebTemplate)]
#[template(path = "controller/partials/ip_pools.html")]
pub struct CtrlIpPoolsPartial {
    pub nwid: String,
    pub network: ControllerNetwork,
    pub pools: Vec<IpAssignmentPool>,
    pub routes: Vec<ControllerRoute>,
    pub can_modify: bool,
}

#[derive(Template, WebTemplate)]
#[template(path = "controller/partials/member_row.html")]
pub struct CtrlMemberRowPartial {
    pub nwid: String,
    pub row: MemberDisplayRow,
    pub can_authorize: bool,
    pub can_modify: bool,
}

#[derive(Template, WebTemplate)]
#[template(path = "controller/partials/member_modal.html")]
pub struct CtrlMemberModalPartial {
    pub nwid: String,
    pub member: ControllerMember,
    pub name: String,
    pub rfc4193_addr: Option<String>,
    pub sixplane_addr: Option<String>,
    pub can_modify: bool,
}

#[derive(Template, WebTemplate)]
#[template(path = "controller/partials/flow_rules.html")]
pub struct CtrlFlowRulesPartial {
    pub nwid: String,
    pub network: ControllerNetwork,
    pub rules_source: String,
    pub can_modify: bool,
}

// ---- Handlers: Pages ----

pub async fn controller_network_detail(
    State(state): State<AppState>,
    Extension(user): Extension<User>,
    Path(nwid): Path<String>,
) -> Response {
    // Check read permission
    if !permissions::can_read(&user, &nwid) {
        return (StatusCode::FORBIDDEN, "You don't have permission to view this network").into_response();
    }

    let client = state.zt_client.read().await;
    let (nw_result, members_result) = match client.as_ref() {
        Some(c) => {
            let nw = c.get_controller_network(&nwid).await;
            let member_ids = c.get_controller_members(&nwid).await;
            let members = match member_ids {
                Ok(ids) => {
                    let mut mems = Vec::new();
                    for mid in ids.keys() {
                        if let Ok(m) = c.get_controller_member(&nwid, mid).await {
                            mems.push(m);
                        }
                    }
                    Ok(mems)
                }
                Err(e) => Err(e),
            };
            (Some(nw), Some(members))
        }
        None => (None, None),
    };
    drop(client);

    let config = state.config.read().await;
    let member_names = config
        .as_ref()
        .map(|c| c.member_names.clone())
        .unwrap_or_default();
    let rules_source = config
        .as_ref()
        .and_then(|c| c.rules_source.get(&nwid).cloned())
        .unwrap_or_default();
    drop(config);

    // Get user permissions for this network
    let can_authorize = permissions::can_authorize(&user, &nwid);
    let can_modify = permissions::can_modify(&user, &nwid);
    let can_delete = permissions::can_delete(&user, &nwid);

    match nw_result {
        Some(Ok(network)) => {
            let members = members_result.and_then(|r| r.ok()).unwrap_or_default();
            let member_count = members.len();
            let authorized_count = members.iter().filter(|m| m.is_authorized()).count();
            let pools = network.ip_assignment_pools.clone();
            let routes = network.routes.clone();
            let rows = enrich_members(&members, &member_names, &network);
            ControllerNetworkDetailTemplate {
                nwid,
                pools,
                routes,
                network,
                rows,
                member_count,
                authorized_count,
                rules_source,
                is_htmx: false,
                version: crate::VERSION,
                can_authorize,
                can_modify,
                can_delete,
            }
            .into_response()
        }
        _ => {
            // Fallback to cached state
            let zt = state.zt_state.read().await;
            if let Some(nw) = zt
                .controller_networks
                .iter()
                .find(|n| n.display_id() == nwid)
            {
                let members = zt
                    .controller_members
                    .get(&nwid)
                    .cloned()
                    .unwrap_or_default();
                let member_count = members.len();
                let authorized_count = members.iter().filter(|m| m.is_authorized()).count();
                let pools = nw.ip_assignment_pools.clone();
                let routes = nw.routes.clone();
                let rows = enrich_members(&members, &member_names, nw);
                ControllerNetworkDetailTemplate {
                    nwid,
                    pools,
                    routes,
                    network: nw.clone(),
                    rows,
                    member_count,
                    authorized_count,
                    rules_source,
                    is_htmx: false,
                    version: crate::VERSION,
                    can_authorize,
                    can_modify,
                    can_delete,
                }
                .into_response()
            } else {
                (StatusCode::NOT_FOUND, "Controller network not found").into_response()
            }
        }
    }
}

// ---- Handlers: Network Actions ----

pub async fn create_network(
    State(state): State<AppState>,
    Extension(user): Extension<User>,
) -> Response {
    // Only admins can create networks
    if !permissions::is_admin(&user) {
        return (StatusCode::FORBIDDEN, "Only administrators can create networks").into_response();
    }

    let zt = state.zt_state.read().await;
    let node_address = match zt.status.as_ref().and_then(|s| s.address.clone()) {
        Some(addr) => addr,
        None => {
            return (
                StatusCode::SERVICE_UNAVAILABLE,
                "Node address not available",
            )
                .into_response()
        }
    };
    drop(zt);

    let client = state.zt_client.read().await;
    let result = match client.as_ref() {
        Some(c) => Some(c.create_controller_network(&node_address).await),
        None => None,
    };

    match result {
        Some(Ok(network)) => {
            let nwid = network.display_id().to_string();

            // Apply default rules to the new network
            if let Some(c) = client.as_ref() {
                let default_rules = default_compiled_rules();
                let _ = c
                    .update_controller_network(
                        &nwid,
                        serde_json::json!({
                            "rules": default_rules
                        }),
                    )
                    .await;
            }
            drop(client);

            // Save default DSL source locally
            let _ = state
                .save_rules_source(&nwid, DEFAULT_RULES_SOURCE)
                .await;

            state.notify_poller();
            Redirect::to(&format!("/controller/{}", nwid)).into_response()
        }
        Some(Err(e)) => {
            drop(client);
            (StatusCode::BAD_GATEWAY, format!("Failed to create: {}", e)).into_response()
        }
        None => {
            drop(client);
            (
                StatusCode::SERVICE_UNAVAILABLE,
                "ZeroTier client not configured",
            )
                .into_response()
        }
    }
}

pub async fn delete_network(
    State(state): State<AppState>,
    Extension(user): Extension<User>,
    Path(nwid): Path<String>,
) -> Response {
    // Check delete permission
    if !permissions::can_delete(&user, &nwid) {
        return (StatusCode::FORBIDDEN, "You don't have permission to delete this network").into_response();
    }

    let client = state.zt_client.read().await;
    let result = match client.as_ref() {
        Some(c) => Some(c.delete_controller_network(&nwid).await),
        None => None,
    };
    drop(client);

    match result {
        Some(Ok(_)) => {
            state.notify_poller();
            // Brief delay to let poller update cached state before redirect
            tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
            Redirect::to("/").into_response()
        }
        Some(Err(e)) => {
            (StatusCode::BAD_GATEWAY, format!("Failed to delete: {}", e)).into_response()
        }
        None => (
            StatusCode::SERVICE_UNAVAILABLE,
            "ZeroTier client not configured",
        )
            .into_response(),
    }
}

// ---- Handlers: Network Settings ----

#[derive(Deserialize)]
pub struct UpdateSettingsForm {
    pub name: Option<String>,
    pub private: Option<String>,
}

pub async fn update_settings(
    State(state): State<AppState>,
    Extension(user): Extension<User>,
    Path(nwid): Path<String>,
    Form(form): Form<UpdateSettingsForm>,
) -> Response {
    // Check modify permission
    if !permissions::can_modify(&user, &nwid) {
        return (StatusCode::FORBIDDEN, "You don't have permission to modify this network").into_response();
    }

    let body = serde_json::json!({
        "name": form.name.unwrap_or_default(),
        "private": form.private.is_some(),
    });

    let client = state.zt_client.read().await;
    let result = match client.as_ref() {
        Some(c) => Some(c.update_controller_network(&nwid, body).await),
        None => None,
    };
    drop(client);

    match result {
        Some(Ok(network)) => {
            state.notify_poller();
            CtrlNetworkSettingsPartial { network, can_modify: true }.into_response()
        }
        Some(Err(e)) => (StatusCode::BAD_GATEWAY, format!("Failed: {}", e)).into_response(),
        None => (StatusCode::SERVICE_UNAVAILABLE, "Not configured").into_response(),
    }
}

// ---- Handlers: Broadcast Settings ----

#[derive(Deserialize)]
pub struct UpdateBroadcastForm {
    pub enable_broadcast: Option<String>,
    pub multicast_limit: Option<u32>,
}

pub async fn update_broadcast_settings(
    State(state): State<AppState>,
    Extension(user): Extension<User>,
    Path(nwid): Path<String>,
    Form(form): Form<UpdateBroadcastForm>,
) -> Response {
    if !permissions::can_modify(&user, &nwid) {
        return (StatusCode::FORBIDDEN, "You don't have permission to modify this network").into_response();
    }

    let body = serde_json::json!({
        "enableBroadcast": form.enable_broadcast.is_some(),
        "multicastLimit": form.multicast_limit.unwrap_or(32),
    });

    let client = state.zt_client.read().await;
    let result = match client.as_ref() {
        Some(c) => Some(c.update_controller_network(&nwid, body).await),
        None => None,
    };
    drop(client);

    match result {
        Some(Ok(network)) => {
            state.notify_poller();
            let pools = network.ip_assignment_pools.clone();
            let routes = network.routes.clone();
            CtrlIpPoolsPartial {
                nwid,
                network,
                pools,
                routes,
                can_modify: true,
            }
            .into_response()
        }
        Some(Err(e)) => (StatusCode::BAD_GATEWAY, format!("Failed: {}", e)).into_response(),
        None => (StatusCode::SERVICE_UNAVAILABLE, "Not configured").into_response(),
    }
}

// ---- Handlers: Assignment Modes ----

#[derive(Deserialize)]
pub struct UpdateAssignModesForm {
    pub v4_auto_assign: Option<String>,
    pub v6_rfc4193: Option<String>,
    pub v6_sixplane: Option<String>,
    pub v6_auto_assign: Option<String>,
}

pub async fn update_assign_modes(
    State(state): State<AppState>,
    Extension(user): Extension<User>,
    Path(nwid): Path<String>,
    Form(form): Form<UpdateAssignModesForm>,
) -> Response {
    if !permissions::can_modify(&user, &nwid) {
        return (StatusCode::FORBIDDEN, "You don't have permission to modify this network").into_response();
    }

    let body = serde_json::json!({
        "v4AssignMode": { "zt": form.v4_auto_assign.is_some() },
        "v6AssignMode": {
            "rfc4193": form.v6_rfc4193.is_some(),
            "6plane": form.v6_sixplane.is_some(),
            "zt": form.v6_auto_assign.is_some()
        },
    });

    let client = state.zt_client.read().await;
    let result = match client.as_ref() {
        Some(c) => Some(c.update_controller_network(&nwid, body).await),
        None => None,
    };
    drop(client);

    match result {
        Some(Ok(network)) => {
            state.notify_poller();
            let pools = network.ip_assignment_pools.clone();
            let routes = network.routes.clone();
            CtrlIpPoolsPartial {
                nwid,
                network,
                pools,
                routes,
                can_modify: true,
            }
            .into_response()
        }
        Some(Err(e)) => (StatusCode::BAD_GATEWAY, format!("Failed: {}", e)).into_response(),
        None => (StatusCode::SERVICE_UNAVAILABLE, "Not configured").into_response(),
    }
}

// ---- Handlers: IP Pools ----

#[derive(Deserialize)]
pub struct AddPoolForm {
    pub range_start: String,
    pub range_end: String,
}

pub async fn add_pool(
    State(state): State<AppState>,
    Extension(user): Extension<User>,
    Path(nwid): Path<String>,
    Form(form): Form<AddPoolForm>,
) -> Response {
    if !permissions::can_modify(&user, &nwid) {
        return (StatusCode::FORBIDDEN, "You don't have permission to modify this network").into_response();
    }

    let client = state.zt_client.read().await;
    let client_ref = match client.as_ref() {
        Some(c) => c.clone(),
        None => return (StatusCode::SERVICE_UNAVAILABLE, "Not configured").into_response(),
    };
    drop(client);

    let current = match client_ref.get_controller_network(&nwid).await {
        Ok(n) => n,
        Err(e) => return (StatusCode::BAD_GATEWAY, format!("Failed: {}", e)).into_response(),
    };

    let mut pools: Vec<serde_json::Value> = current
        .ip_assignment_pools
        .iter()
        .map(|p| {
            serde_json::json!({"ipRangeStart": p.ip_range_start, "ipRangeEnd": p.ip_range_end})
        })
        .collect();
    pools.push(serde_json::json!({
        "ipRangeStart": form.range_start.trim(),
        "ipRangeEnd": form.range_end.trim(),
    }));

    let body = serde_json::json!({"ipAssignmentPools": pools});
    match client_ref.update_controller_network(&nwid, body).await {
        Ok(network) => {
            state.notify_poller();
            let pools = network.ip_assignment_pools.clone();
            let routes = network.routes.clone();
            CtrlIpPoolsPartial {
                nwid,
                network,
                pools,
                routes,
                can_modify: true,
            }
            .into_response()
        }
        Err(e) => (StatusCode::BAD_GATEWAY, format!("Failed: {}", e)).into_response(),
    }
}

#[derive(Deserialize)]
pub struct RemovePoolForm {
    pub index: usize,
}

pub async fn remove_pool(
    State(state): State<AppState>,
    Extension(user): Extension<User>,
    Path(nwid): Path<String>,
    Form(form): Form<RemovePoolForm>,
) -> Response {
    if !permissions::can_modify(&user, &nwid) {
        return (StatusCode::FORBIDDEN, "You don't have permission to modify this network").into_response();
    }

    let client = state.zt_client.read().await;
    let client_ref = match client.as_ref() {
        Some(c) => c.clone(),
        None => return (StatusCode::SERVICE_UNAVAILABLE, "Not configured").into_response(),
    };
    drop(client);

    let current = match client_ref.get_controller_network(&nwid).await {
        Ok(n) => n,
        Err(e) => return (StatusCode::BAD_GATEWAY, format!("Failed: {}", e)).into_response(),
    };

    let pools: Vec<serde_json::Value> = current
        .ip_assignment_pools
        .iter()
        .enumerate()
        .filter(|(i, _)| *i != form.index)
        .map(|(_, p)| {
            serde_json::json!({"ipRangeStart": p.ip_range_start, "ipRangeEnd": p.ip_range_end})
        })
        .collect();

    let body = serde_json::json!({"ipAssignmentPools": pools});
    match client_ref.update_controller_network(&nwid, body).await {
        Ok(network) => {
            state.notify_poller();
            let pools = network.ip_assignment_pools.clone();
            let routes = network.routes.clone();
            CtrlIpPoolsPartial {
                nwid,
                network,
                pools,
                routes,
                can_modify: true,
            }
            .into_response()
        }
        Err(e) => (StatusCode::BAD_GATEWAY, format!("Failed: {}", e)).into_response(),
    }
}

// ---- Handlers: Routes ----

#[derive(Deserialize)]
pub struct AddRouteForm {
    pub target: String,
    pub via: Option<String>,
}

pub async fn add_route(
    State(state): State<AppState>,
    Extension(user): Extension<User>,
    Path(nwid): Path<String>,
    Form(form): Form<AddRouteForm>,
) -> Response {
    if !permissions::can_modify(&user, &nwid) {
        return (StatusCode::FORBIDDEN, "You don't have permission to modify this network").into_response();
    }

    let client = state.zt_client.read().await;
    let client_ref = match client.as_ref() {
        Some(c) => c.clone(),
        None => return (StatusCode::SERVICE_UNAVAILABLE, "Not configured").into_response(),
    };
    drop(client);

    let current = match client_ref.get_controller_network(&nwid).await {
        Ok(n) => n,
        Err(e) => return (StatusCode::BAD_GATEWAY, format!("Failed: {}", e)).into_response(),
    };

    let mut routes: Vec<serde_json::Value> = current
        .routes
        .iter()
        .map(|r| serde_json::json!({"target": r.target, "via": r.via}))
        .collect();
    let via = form
        .via
        .as_deref()
        .filter(|s| !s.trim().is_empty())
        .map(|s| s.trim().to_string());
    routes.push(serde_json::json!({"target": form.target.trim(), "via": via}));

    let body = serde_json::json!({"routes": routes});
    match client_ref.update_controller_network(&nwid, body).await {
        Ok(network) => {
            state.notify_poller();
            let pools = network.ip_assignment_pools.clone();
            let routes = network.routes.clone();
            CtrlIpPoolsPartial {
                nwid,
                network,
                pools,
                routes,
                can_modify: true,
            }
            .into_response()
        }
        Err(e) => (StatusCode::BAD_GATEWAY, format!("Failed: {}", e)).into_response(),
    }
}

#[derive(Deserialize)]
pub struct RemoveRouteForm {
    pub index: usize,
}

pub async fn remove_route(
    State(state): State<AppState>,
    Extension(user): Extension<User>,
    Path(nwid): Path<String>,
    Form(form): Form<RemoveRouteForm>,
) -> Response {
    if !permissions::can_modify(&user, &nwid) {
        return (StatusCode::FORBIDDEN, "You don't have permission to modify this network").into_response();
    }

    let client = state.zt_client.read().await;
    let client_ref = match client.as_ref() {
        Some(c) => c.clone(),
        None => return (StatusCode::SERVICE_UNAVAILABLE, "Not configured").into_response(),
    };
    drop(client);

    let current = match client_ref.get_controller_network(&nwid).await {
        Ok(n) => n,
        Err(e) => return (StatusCode::BAD_GATEWAY, format!("Failed: {}", e)).into_response(),
    };

    let routes: Vec<serde_json::Value> = current
        .routes
        .iter()
        .enumerate()
        .filter(|(i, _)| *i != form.index)
        .map(|(_, r)| serde_json::json!({"target": r.target, "via": r.via}))
        .collect();

    let body = serde_json::json!({"routes": routes});
    match client_ref.update_controller_network(&nwid, body).await {
        Ok(network) => {
            state.notify_poller();
            let pools = network.ip_assignment_pools.clone();
            let routes = network.routes.clone();
            CtrlIpPoolsPartial {
                nwid,
                network,
                pools,
                routes,
                can_modify: true,
            }
            .into_response()
        }
        Err(e) => (StatusCode::BAD_GATEWAY, format!("Failed: {}", e)).into_response(),
    }
}

// ---- Handlers: DNS ----

#[derive(Deserialize)]
pub struct AddDnsForm {
    pub domain: Option<String>,
    pub server: String,
}

pub async fn add_dns(
    State(state): State<AppState>,
    Extension(user): Extension<User>,
    Path(nwid): Path<String>,
    Form(form): Form<AddDnsForm>,
) -> Response {
    if !permissions::can_modify(&user, &nwid) {
        return (StatusCode::FORBIDDEN, "You don't have permission to modify this network").into_response();
    }

    let client = state.zt_client.read().await;
    let client_ref = match client.as_ref() {
        Some(c) => c.clone(),
        None => return (StatusCode::SERVICE_UNAVAILABLE, "Not configured").into_response(),
    };
    drop(client);

    let current = match client_ref.get_controller_network(&nwid).await {
        Ok(n) => n,
        Err(e) => return (StatusCode::BAD_GATEWAY, format!("Failed: {}", e)).into_response(),
    };

    let mut servers = current.dns.servers.clone();
    let server = form.server.trim().to_string();
    if !server.is_empty() && !servers.contains(&server) {
        servers.push(server);
    }

    let domain = form.domain
        .as_ref()
        .map(|d| d.trim())
        .filter(|d| !d.is_empty())
        .unwrap_or(&current.dns.domain)
        .to_string();

    let body = serde_json::json!({
        "dns": {
            "domain": domain,
            "servers": servers,
        }
    });

    match client_ref.update_controller_network(&nwid, body).await {
        Ok(network) => {
            state.notify_poller();
            let pools = network.ip_assignment_pools.clone();
            let routes = network.routes.clone();
            CtrlIpPoolsPartial {
                nwid,
                network,
                pools,
                routes,
                can_modify: true,
            }
            .into_response()
        }
        Err(e) => (StatusCode::BAD_GATEWAY, format!("Failed: {}", e)).into_response(),
    }
}

#[derive(Deserialize)]
pub struct RemoveDnsForm {
    pub index: usize,
}

pub async fn remove_dns(
    State(state): State<AppState>,
    Extension(user): Extension<User>,
    Path(nwid): Path<String>,
    Form(form): Form<RemoveDnsForm>,
) -> Response {
    if !permissions::can_modify(&user, &nwid) {
        return (StatusCode::FORBIDDEN, "You don't have permission to modify this network").into_response();
    }

    let client = state.zt_client.read().await;
    let client_ref = match client.as_ref() {
        Some(c) => c.clone(),
        None => return (StatusCode::SERVICE_UNAVAILABLE, "Not configured").into_response(),
    };
    drop(client);

    let current = match client_ref.get_controller_network(&nwid).await {
        Ok(n) => n,
        Err(e) => return (StatusCode::BAD_GATEWAY, format!("Failed: {}", e)).into_response(),
    };

    let servers: Vec<String> = current
        .dns
        .servers
        .iter()
        .enumerate()
        .filter(|(i, _)| *i != form.index)
        .map(|(_, s)| s.clone())
        .collect();

    // Clear domain if no servers left
    let domain = if servers.is_empty() {
        String::new()
    } else {
        current.dns.domain.clone()
    };

    let body = serde_json::json!({
        "dns": {
            "domain": domain,
            "servers": servers,
        }
    });

    match client_ref.update_controller_network(&nwid, body).await {
        Ok(network) => {
            state.notify_poller();
            let pools = network.ip_assignment_pools.clone();
            let routes = network.routes.clone();
            CtrlIpPoolsPartial {
                nwid,
                network,
                pools,
                routes,
                can_modify: true,
            }
            .into_response()
        }
        Err(e) => (StatusCode::BAD_GATEWAY, format!("Failed: {}", e)).into_response(),
    }
}

// ---- Handlers: Member Actions ----

pub async fn toggle_member_auth(
    State(state): State<AppState>,
    Extension(user): Extension<User>,
    Path((nwid, member_id)): Path<(String, String)>,
) -> Response {
    if !permissions::can_authorize(&user, &nwid) {
        return (StatusCode::FORBIDDEN, "You don't have permission to authorize members").into_response();
    }

    let client = state.zt_client.read().await;
    let client_ref = match client.as_ref() {
        Some(c) => c.clone(),
        None => return (StatusCode::SERVICE_UNAVAILABLE, "Not configured").into_response(),
    };
    drop(client);

    let current = match client_ref.get_controller_member(&nwid, &member_id).await {
        Ok(m) => m,
        Err(e) => return (StatusCode::BAD_GATEWAY, format!("Failed: {}", e)).into_response(),
    };

    let network = match client_ref.get_controller_network(&nwid).await {
        Ok(n) => n,
        Err(e) => return (StatusCode::BAD_GATEWAY, format!("Failed: {}", e)).into_response(),
    };

    let new_auth = !current.is_authorized();
    let body = serde_json::json!({"authorized": new_auth});
    match client_ref
        .update_controller_member(&nwid, &member_id, body)
        .await
    {
        Ok(member) => {
            state.notify_poller();
            let config = state.config.read().await;
            let member_names = config
                .as_ref()
                .map(|c| c.member_names.clone())
                .unwrap_or_default();
            drop(config);
            let rows = enrich_members(&[member], &member_names, &network);
            CtrlMemberRowPartial {
                nwid: nwid.clone(),
                row: rows.into_iter().next().unwrap(),
                can_authorize: permissions::can_authorize(&user, &nwid),
                can_modify: permissions::can_modify(&user, &nwid),
            }
            .into_response()
        }
        Err(e) => (StatusCode::BAD_GATEWAY, format!("Failed: {}", e)).into_response(),
    }
}

pub async fn delete_member(
    State(state): State<AppState>,
    Extension(user): Extension<User>,
    Path((nwid, member_id)): Path<(String, String)>,
) -> Response {
    if !permissions::can_modify(&user, &nwid) {
        return (StatusCode::FORBIDDEN, "You don't have permission to remove members").into_response();
    }

    let client = state.zt_client.read().await;
    let result = match client.as_ref() {
        Some(c) => Some(c.delete_controller_member(&nwid, &member_id).await),
        None => None,
    };
    drop(client);

    match result {
        Some(Ok(_)) => {
            state.notify_poller();
            (StatusCode::OK, "").into_response()
        }
        Some(Err(e)) => {
            (StatusCode::BAD_GATEWAY, format!("Failed: {}", e)).into_response()
        }
        None => (StatusCode::SERVICE_UNAVAILABLE, "Not configured").into_response(),
    }
}

// ---- Handlers: Add Member ----

#[derive(Deserialize)]
pub struct AddMemberForm {
    pub node_id: String,
}

pub async fn add_member(
    State(state): State<AppState>,
    Extension(user): Extension<User>,
    Path(nwid): Path<String>,
    Form(form): Form<AddMemberForm>,
) -> Response {
    if !permissions::can_authorize(&user, &nwid) {
        return (StatusCode::FORBIDDEN, "You don't have permission to add members").into_response();
    }

    let node_id = form.node_id.trim().to_lowercase();

    // Validate: 10 hex characters
    if node_id.len() != 10 || !node_id.chars().all(|c| c.is_ascii_hexdigit()) {
        return (StatusCode::BAD_REQUEST, "Node ID must be 10 hex characters").into_response();
    }

    let client = state.zt_client.read().await;
    let client_ref = match client.as_ref() {
        Some(c) => c.clone(),
        None => return (StatusCode::SERVICE_UNAVAILABLE, "Not configured").into_response(),
    };
    drop(client);

    // Creating a member by POSTing to the member endpoint with authorized: false
    let body = serde_json::json!({"authorized": false});
    if let Err(e) = client_ref
        .update_controller_member(&nwid, &node_id, body)
        .await
    {
        return (StatusCode::BAD_GATEWAY, format!("Failed: {}", e)).into_response();
    }

    state.notify_poller();

    // Fetch fresh member list (the newly added member won't be in poller cache yet)
    let config = state.config.read().await;
    let member_names = config
        .as_ref()
        .map(|c| c.member_names.clone())
        .unwrap_or_default();
    drop(config);

    let network = match client_ref.get_controller_network(&nwid).await {
        Ok(n) => n,
        Err(e) => return (StatusCode::BAD_GATEWAY, format!("Failed: {}", e)).into_response(),
    };

    let member_ids = client_ref.get_controller_members(&nwid).await;
    let fresh_members = match member_ids {
        Ok(ids) => {
            let mut mems = Vec::new();
            for mid in ids.keys() {
                if let Ok(m) = client_ref.get_controller_member(&nwid, mid).await {
                    mems.push(m);
                }
            }
            mems.sort_by(|a, b| a.display_id().cmp(b.display_id()));
            mems
        }
        Err(_) => vec![],
    };

    let member_count = fresh_members.len();
    let authorized_count = fresh_members.iter().filter(|m| m.is_authorized()).count();
    let rows = enrich_members(&fresh_members, &member_names, &network);
    CtrlMemberListPartial {
        nwid: nwid.clone(),
        rows,
        member_count,
        authorized_count,
        is_htmx: true,
        can_authorize: permissions::can_authorize(&user, &nwid),
        can_modify: permissions::can_modify(&user, &nwid),
    }.into_response()
}

// ---- Handlers: Member Modal ----

pub async fn member_modal(
    State(state): State<AppState>,
    Extension(user): Extension<User>,
    Path((nwid, member_id)): Path<(String, String)>,
) -> Response {
    if !permissions::can_read(&user, &nwid) {
        return (StatusCode::FORBIDDEN, "You don't have permission to view this network").into_response();
    }

    let client = state.zt_client.read().await;
    let client_ref = match client.as_ref() {
        Some(c) => c.clone(),
        None => return (StatusCode::SERVICE_UNAVAILABLE, "Not configured").into_response(),
    };
    drop(client);

    let member = match client_ref.get_controller_member(&nwid, &member_id).await {
        Ok(m) => m,
        Err(e) => return (StatusCode::BAD_GATEWAY, format!("Failed: {}", e)).into_response(),
    };

    let network = match client_ref.get_controller_network(&nwid).await {
        Ok(n) => n,
        Err(e) => return (StatusCode::BAD_GATEWAY, format!("Failed: {}", e)).into_response(),
    };

    let config = state.config.read().await;
    let name = config
        .as_ref()
        .and_then(|c| c.member_names.get(&member_id).cloned())
        .unwrap_or_default();
    drop(config);

    let rfc4193_addr = if network.v6_rfc4193() { member.rfc4193_address() } else { None };
    let sixplane_addr = if network.v6_sixplane() { member.sixplane_address() } else { None };
    let can_modify = permissions::can_modify(&user, &nwid);

    CtrlMemberModalPartial {
        nwid,
        member,
        name,
        rfc4193_addr,
        sixplane_addr,
        can_modify,
    }
    .into_response()
}

// ---- Handlers: Update Member (from modal) ----

#[derive(Deserialize)]
pub struct UpdateMemberForm {
    pub name: Option<String>,
    pub authorized: Option<String>,
    pub active_bridge: Option<String>,
    pub no_auto_assign_ips: Option<String>,
    pub ip_assignments: Option<String>,
}

pub async fn update_member(
    State(state): State<AppState>,
    Extension(user): Extension<User>,
    Path((nwid, member_id)): Path<(String, String)>,
    Form(form): Form<UpdateMemberForm>,
) -> Response {
    if !permissions::can_modify(&user, &nwid) {
        return (StatusCode::FORBIDDEN, "You don't have permission to modify members").into_response();
    }

    // Save name locally
    let name = form.name.as_deref().unwrap_or("").trim().to_string();
    if let Err(e) = state.save_member_name(&member_id, &name).await {
        return (StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to save name: {}", e))
            .into_response();
    }

    // Parse IP assignments: comma or newline separated
    let ip_list: Vec<String> = form
        .ip_assignments
        .as_deref()
        .unwrap_or("")
        .split(|c: char| c == ',' || c == '\n')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();

    // Update member via ZT API
    let body = serde_json::json!({
        "authorized": form.authorized.is_some(),
        "activeBridge": form.active_bridge.is_some(),
        "noAutoAssignIps": form.no_auto_assign_ips.is_some(),
        "ipAssignments": ip_list,
    });

    let client = state.zt_client.read().await;
    let client_ref = match client.as_ref() {
        Some(c) => c.clone(),
        None => return (StatusCode::SERVICE_UNAVAILABLE, "Not configured").into_response(),
    };
    drop(client);

    match client_ref
        .update_controller_member(&nwid, &member_id, body)
        .await
    {
        Ok(_) => {
            state.notify_poller();
            // Return empty response with HX-Trigger to close modal and refresh
            Response::builder()
                .status(StatusCode::OK)
                .header("HX-Trigger", "member-updated")
                .body(axum::body::Body::empty())
                .unwrap()
                .into_response()
        }
        Err(e) => (StatusCode::BAD_GATEWAY, format!("Failed: {}", e)).into_response(),
    }
}

// ---- Handlers: SSE Partials ----

pub async fn ctrl_member_list_partial(
    State(state): State<AppState>,
    Extension(user): Extension<User>,
    Path(nwid): Path<String>,
) -> Response {
    if !permissions::can_read(&user, &nwid) {
        return (StatusCode::FORBIDDEN, "You don't have permission to view this network").into_response();
    }

    let zt = state.zt_state.read().await;
    let network = zt
        .controller_networks
        .iter()
        .find(|n| n.display_id() == nwid)
        .cloned()
        .unwrap_or_default();
    let members = zt
        .controller_members
        .get(&nwid)
        .cloned()
        .unwrap_or_default();
    drop(zt);

    let config = state.config.read().await;
    let member_names = config
        .as_ref()
        .map(|c| c.member_names.clone())
        .unwrap_or_default();
    drop(config);

    let member_count = members.len();
    let authorized_count = members.iter().filter(|m| m.is_authorized()).count();
    let rows = enrich_members(&members, &member_names, &network);
    CtrlMemberListPartial {
        nwid: nwid.clone(),
        rows,
        member_count,
        authorized_count,
        is_htmx: true,
        can_authorize: permissions::can_authorize(&user, &nwid),
        can_modify: permissions::can_modify(&user, &nwid),
    }.into_response()
}

// ---- Handlers: Flow Rules ----

#[derive(Deserialize)]
pub struct UpdateFlowRulesForm {
    pub rules_source: String,
    pub compiled_rules: String,
}

#[derive(Deserialize)]
struct CompiledRules {
    rules: Vec<serde_json::Value>,
    #[serde(default)]
    capabilities: Vec<serde_json::Value>,
    #[serde(default)]
    tags: Vec<serde_json::Value>,
}

pub async fn update_flow_rules(
    State(state): State<AppState>,
    Extension(user): Extension<User>,
    Path(nwid): Path<String>,
    Form(form): Form<UpdateFlowRulesForm>,
) -> Response {
    if !permissions::can_modify(&user, &nwid) {
        return (StatusCode::FORBIDDEN, "You don't have permission to modify this network").into_response();
    }

    // Parse the compiled rules JSON from the hidden field
    let compiled: CompiledRules = match serde_json::from_str(&form.compiled_rules) {
        Ok(r) => r,
        Err(e) => {
            return (StatusCode::BAD_REQUEST, format!("Invalid compiled rules: {}", e)).into_response();
        }
    };

    // Build the body with rules, capabilities, and tags
    let body = serde_json::json!({
        "rules": compiled.rules,
        "capabilities": compiled.capabilities,
        "tags": compiled.tags,
    });

    let client = state.zt_client.read().await;
    let result = match client.as_ref() {
        Some(c) => Some(c.update_controller_network(&nwid, body).await),
        None => None,
    };
    drop(client);

    match result {
        Some(Ok(network)) => {
            state.notify_poller();
            // Save the DSL source locally (ZT API only stores compiled JSON)
            if let Err(e) = state.save_rules_source(&nwid, &form.rules_source).await {
                tracing::warn!("Failed to save rules source: {}", e);
            }
            let rules_source = form.rules_source;
            CtrlFlowRulesPartial { nwid, network, rules_source, can_modify: true }.into_response()
        }
        Some(Err(e)) => (StatusCode::BAD_GATEWAY, format!("Failed: {}", e)).into_response(),
        None => (StatusCode::SERVICE_UNAVAILABLE, "Not configured").into_response(),
    }
}
