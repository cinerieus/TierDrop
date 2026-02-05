use serde::{Deserialize, Serialize};

fn format_epoch_ms(ts: Option<f64>) -> String {
    match ts {
        Some(ms) if ms > 0.0 => {
            let secs = (ms / 1000.0) as u64;
            let dur = std::time::Duration::from_secs(secs);
            let dt = std::time::UNIX_EPOCH + dur;
            let elapsed = dt
                .elapsed()
                .unwrap_or(std::time::Duration::ZERO)
                .as_secs();
            if elapsed < 60 {
                "just now".to_string()
            } else if elapsed < 3600 {
                format!("{}m ago", elapsed / 60)
            } else if elapsed < 86400 {
                format!("{}h ago", elapsed / 3600)
            } else {
                format!("{}d ago", elapsed / 86400)
            }
        }
        _ => "-".to_string(),
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct NodeStatus {
    pub address: Option<String>,
    pub public_identity: Option<String>,
    pub online: Option<bool>,
    pub tcp_fallback_active: Option<bool>,
    pub version: Option<String>,
    pub clock: Option<i64>,
    #[serde(default)]
    pub config: serde_json::Value,
}

impl NodeStatus {
    pub fn display_address(&self) -> &str {
        self.address.as_deref().unwrap_or("-")
    }

    pub fn display_version(&self) -> &str {
        self.version.as_deref().unwrap_or("-")
    }

    pub fn is_online(&self) -> bool {
        self.online.unwrap_or(false)
    }
}

// ---- Controller Models ----

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ControllerNetwork {
    pub id: Option<String>,
    pub nwid: Option<String>,
    pub name: Option<String>,
    pub private: Option<bool>,
    pub enable_broadcast: Option<bool>,
    pub v4_assign_mode: Option<V4AssignMode>,
    pub v6_assign_mode: Option<V6AssignMode>,
    pub mtu: Option<u32>,
    pub multicast_limit: Option<u32>,
    pub creation_time: Option<f64>,
    pub revision: Option<u64>,
    #[serde(default)]
    pub routes: Vec<ControllerRoute>,
    #[serde(default)]
    pub ip_assignment_pools: Vec<IpAssignmentPool>,
    #[serde(default)]
    pub rules: Vec<serde_json::Value>,
    #[serde(default)]
    pub dns: serde_json::Value,
}

impl ControllerNetwork {
    pub fn display_id(&self) -> &str {
        self.nwid
            .as_deref()
            .or(self.id.as_deref())
            .unwrap_or("unknown")
    }

    pub fn display_name(&self) -> &str {
        self.name
            .as_deref()
            .filter(|n| !n.is_empty())
            .unwrap_or("Unnamed Network")
    }

    pub fn is_private(&self) -> bool {
        self.private.unwrap_or(true)
    }

    pub fn display_type(&self) -> &str {
        if self.is_private() {
            "Private"
        } else {
            "Public"
        }
    }

    pub fn type_class(&self) -> &str {
        if self.is_private() {
            "type-private"
        } else {
            "type-public"
        }
    }

    pub fn _display_mtu(&self) -> String {
        self.mtu
            .map(|m| m.to_string())
            .unwrap_or_else(|| "2800".to_string())
    }

    pub fn _display_multicast_limit(&self) -> String {
        self.multicast_limit
            .map(|m| m.to_string())
            .unwrap_or_else(|| "32".to_string())
    }

    pub fn v4_auto_assign(&self) -> bool {
        self.v4_assign_mode.as_ref().map(|m| m.zt).unwrap_or(false)
    }

    pub fn broadcast_enabled(&self) -> bool {
        self.enable_broadcast.unwrap_or(false)
    }

    pub fn display_subnet(&self) -> &str {
        self.routes
            .first()
            .and_then(|r| r.target.as_deref())
            .unwrap_or("-")
    }

    pub fn display_creation_time(&self) -> String {
        format_epoch_ms(self.creation_time)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct V4AssignMode {
    #[serde(default)]
    pub zt: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct V6AssignMode {
    #[serde(default, rename = "6plane")]
    pub sixplane: bool,
    #[serde(default)]
    pub rfc4193: bool,
    #[serde(default)]
    pub zt: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct ControllerRoute {
    pub target: Option<String>,
    pub via: Option<String>,
}

impl ControllerRoute {
    pub fn display_target(&self) -> &str {
        self.target.as_deref().unwrap_or("-")
    }

    pub fn display_via(&self) -> &str {
        self.via.as_deref().unwrap_or("(LAN)")
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct IpAssignmentPool {
    pub ip_range_start: Option<String>,
    pub ip_range_end: Option<String>,
}

impl IpAssignmentPool {
    pub fn display_start(&self) -> &str {
        self.ip_range_start.as_deref().unwrap_or("-")
    }

    pub fn display_end(&self) -> &str {
        self.ip_range_end.as_deref().unwrap_or("-")
    }

    pub fn _display_range(&self) -> String {
        match (&self.ip_range_start, &self.ip_range_end) {
            (Some(start), Some(end)) => format!("{} - {}", start, end),
            _ => "-".to_string(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ControllerMember {
    pub id: Option<String>,
    pub address: Option<String>,
    pub nwid: Option<String>,
    pub authorized: Option<bool>,
    pub active_bridge: Option<bool>,
    pub identity: Option<String>,
    #[serde(default)]
    pub ip_assignments: Vec<String>,
    pub revision: Option<u64>,
    pub v_major: Option<i32>,
    pub v_minor: Option<i32>,
    pub v_rev: Option<i32>,
    pub v_proto: Option<i32>,
    #[serde(default)]
    pub no_auto_assign_ips: bool,
    pub creation_time: Option<f64>,
    pub last_authorized_time: Option<f64>,
    pub last_deauthorized_time: Option<f64>,
}

impl ControllerMember {
    pub fn display_id(&self) -> &str {
        self.address
            .as_deref()
            .or(self.id.as_deref())
            .unwrap_or("unknown")
    }

    pub fn is_authorized(&self) -> bool {
        self.authorized.unwrap_or(false)
    }

    pub fn auth_class(&self) -> &str {
        if self.is_authorized() {
            "status-ok"
        } else {
            "status-denied"
        }
    }

    pub fn auth_label(&self) -> &str {
        if self.is_authorized() {
            "Authorized"
        } else {
            "Not Authorized"
        }
    }

    pub fn is_bridge(&self) -> bool {
        self.active_bridge.unwrap_or(false)
    }

    pub fn display_version(&self) -> String {
        match (self.v_major, self.v_minor, self.v_rev) {
            (Some(maj), Some(min), Some(rev)) if maj >= 0 && min >= 0 && rev >= 0 => {
                format!("{}.{}.{}", maj, min, rev)
            }
            _ => "-".to_string(),
        }
    }

    pub fn display_creation_time(&self) -> String {
        format_epoch_ms(self.creation_time)
    }

    pub fn display_last_authorized(&self) -> String {
        format_epoch_ms(self.last_authorized_time)
    }

    pub fn display_last_deauthorized(&self) -> String {
        format_epoch_ms(self.last_deauthorized_time)
    }
}

/// Cached snapshot of all ZeroTier state
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ZtState {
    pub status: Option<NodeStatus>,
    pub controller_networks: Vec<ControllerNetwork>,
    pub controller_members: std::collections::HashMap<String, Vec<ControllerMember>>,
    pub last_updated: Option<std::time::SystemTime>,
    pub error: Option<String>,
}
