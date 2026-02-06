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
    pub capabilities: Vec<serde_json::Value>,
    #[serde(default)]
    pub tags: Vec<serde_json::Value>,
    #[serde(default)]
    pub dns: DnsConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct DnsConfig {
    #[serde(default)]
    pub domain: String,
    #[serde(default)]
    pub servers: Vec<String>,
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

    pub fn display_multicast_limit(&self) -> u32 {
        self.multicast_limit.unwrap_or(32)
    }

    pub fn v4_auto_assign(&self) -> bool {
        self.v4_assign_mode.as_ref().map(|m| m.zt).unwrap_or(false)
    }

    pub fn v6_rfc4193(&self) -> bool {
        self.v6_assign_mode.as_ref().map(|m| m.rfc4193).unwrap_or(false)
    }

    pub fn v6_sixplane(&self) -> bool {
        self.v6_assign_mode.as_ref().map(|m| m.sixplane).unwrap_or(false)
    }

    pub fn v6_zt_auto_assign(&self) -> bool {
        self.v6_assign_mode.as_ref().map(|m| m.zt).unwrap_or(false)
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

    /// Returns the rules, capabilities, and tags as formatted JSON string
    pub fn display_rules_json(&self) -> String {
        let output = serde_json::json!({
            "rules": &self.rules,
            "capabilities": &self.capabilities,
            "tags": &self.tags
        });
        serde_json::to_string_pretty(&output).unwrap_or_else(|_| r#"{"rules":[],"capabilities":[],"tags":[]}"#.to_string())
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

    pub fn is_ipv6(&self) -> bool {
        self.target.as_ref().map(|s| s.contains(':')).unwrap_or(false)
    }

    pub fn is_ipv4(&self) -> bool {
        !self.is_ipv6()
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

    pub fn is_ipv6(&self) -> bool {
        self.ip_range_start.as_ref().map(|s| s.contains(':')).unwrap_or(false)
    }

    pub fn is_ipv4(&self) -> bool {
        !self.is_ipv6()
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

    /// Returns IP assignments as comma-separated string, IPv4 first then IPv6
    pub fn display_ip_assignments(&self) -> String {
        let mut ipv4: Vec<&str> = Vec::new();
        let mut ipv6: Vec<&str> = Vec::new();
        for ip in &self.ip_assignments {
            if ip.contains(':') {
                ipv6.push(ip);
            } else {
                ipv4.push(ip);
            }
        }
        ipv4.extend(ipv6);
        ipv4.join(", ")
    }

    /// Compute RFC4193 address for this member
    /// Format: fd<nwid>9993<nodeid> split into groups of 4
    pub fn rfc4193_address(&self) -> Option<String> {
        let nwid = self.nwid.as_ref()?;
        let node = self.address.as_ref().or(self.id.as_ref())?;
        if nwid.len() != 16 || node.len() != 10 {
            return None;
        }
        // fd + nwid(16) + 9993 + node(10) = 32 hex chars after fd
        let full = format!("fd{}9993{}", nwid, node);
        // Split into groups of 4
        let parts: Vec<&str> = (0..8).map(|i| &full[i * 4..(i + 1) * 4]).collect();
        Some(parts.join(":"))
    }

    /// Compute 6PLANE address for this member
    /// XOR first 4 bytes of nwid with bytes 4-7, then append node + padding
    pub fn sixplane_address(&self) -> Option<String> {
        let nwid = self.nwid.as_ref()?;
        let node = self.address.as_ref().or(self.id.as_ref())?;
        if nwid.len() != 16 || node.len() != 10 {
            return None;
        }
        // Parse nwid bytes
        let nwid_bytes: Vec<u8> = (0..8)
            .filter_map(|i| u8::from_str_radix(&nwid[i * 2..i * 2 + 2], 16).ok())
            .collect();
        if nwid_bytes.len() != 8 {
            return None;
        }
        // XOR first 4 bytes with bytes 4-7
        let xored: Vec<String> = (0..4)
            .map(|i| format!("{:02x}", nwid_bytes[i] ^ nwid_bytes[i + 4]))
            .collect();
        // Build: fc(2) + xored(8) + node(10) + padding(12) = 32 hex chars
        let full = format!("fc{}{}{}", xored.join(""), node, "000000000001");
        // Split into groups of 4
        let parts: Vec<&str> = (0..8).map(|i| &full[i * 4..(i + 1) * 4]).collect();
        Some(parts.join(":"))
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
